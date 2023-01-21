use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::Base64VecU8;
use near_sdk::json_types::{ValidAccountId, U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    env, near_bindgen, serde_json, AccountId, Balance, Gas, Promise, PromiseResult, Timestamp,
};
near_sdk::setup_alloc!();

const NO_DEPOSIT: Balance = 0;
const YOCTO_DEPOSIT: Balance = 1;
// Fiat values with two decimals
const ONE_FIAT: Balance = 100;
const MIN_GAS: Gas = 50_000_000_000_000;
const BASIC_GAS: Gas = 10_000_000_000_000;

// Helper struct for serializing / deserializing the `msg` argument of `ft_on_transfer`
#[derive(Serialize, Deserialize)]
struct TransferWithReferenceArgs {
    payment_reference: String,
    to: ValidAccountId,
    amount: U128,
    currency: String,
    token_address: ValidAccountId,
    fee_address: ValidAccountId,
    fee_amount: U128,
    max_rate_timespan: U64,
}

/**
 * Fungible token-related declarations
 */

// Return type the fungible token metadata
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<Base64VecU8>,
    pub decimals: u8,
}

// Interface of fungible tokens
#[near_sdk::ext_contract(fungible_token)]
trait FungibleTokenContract {
    fn ft_transfer(receiver_id: String, amount: String, memo: Option<String>);

    fn ft_metadata() -> Promise<FungibleTokenMetadata>;
}

/**
 * Flux oracle-related declarations
 */

// Return type the Flux price oracle
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct PriceEntry {
    pub price: U128,            // Last reported price
    pub decimals: u16,          // Amount of decimals (e.g. if 2, 100 = 1.00)
    pub last_update: Timestamp, // Time of report
}

// Interface of the Flux price oracle
#[near_sdk::ext_contract(fpo_contract)]
trait FPOContract {
    fn get_entry(pair: String, provider: AccountId) -> Promise<PriceEntry>;
}

///
/// This contract
/// - oracle_account_id: should be a valid FPO oracle account ID
/// - provider_account_id: should be a valid FPO provider account ID
/// - owner_id: only the owner can edit the contract state values above (default = deployer)
#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct FungibleConversionProxy {
    pub oracle_account_id: AccountId,
    pub provider_account_id: AccountId,
    pub owner_id: AccountId,
}

// Callback methods
#[near_sdk::ext_contract(ext_self)]
pub trait ExtSelfRequestProxy {
    fn on_transfer_with_reference(
        &self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        fee_address: ValidAccountId,
        fee_amount: U128,
        crypto_amount: U128,
        crypto_fee_amount: U128,
        max_rate_timespan: U64,
        deposit: U128,
        change: U128,
        predecessor_account_id: AccountId,
        token_address: ValidAccountId,
    ) -> bool;

    fn ft_metadata_callback(
        &self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payer: AccountId,
        deposit: U128,
    ) -> u128;

    fn rate_callback(
        &self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payer: AccountId,
        deposit: U128,
        payment_token_decimals: u8,
    ) -> u128;
}

trait FungibleTokenReceiver {
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> String;
}

#[near_bindgen]
impl FungibleTokenReceiver for FungibleConversionProxy {
    ///
    /// This is the function that will be called by the fungible token contract's `ft_transfer_call` function.
    ///
    /// Eg. msg = {"payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near","amount":"1000000","currency":"USD","token_address":"a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near","fee_address":"fee.requestfinance.near","fee_amount":"200","max_rate_timespan":"0"}
    ///
    /// For more information on the fungible token standard, see https://nomicon.io/Standards/Tokens/FungibleToken/Core
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> String {
        let args: TransferWithReferenceArgs = serde_json::from_str(&msg).unwrap();

        self.transfer_with_reference(
            args.payment_reference,
            args.to,
            args.amount,
            args.currency,
            args.token_address,
            args.fee_address,
            args.fee_amount,
            args.max_rate_timespan,
            sender_id,
            amount,
        );

        // We cannot determine the refund until the cross-contract calls for the FT metadata and oracle
        // complete, so we always intially refund 0 and handle refunds afterwards
        "0".into()
    }
}

#[near_bindgen]
impl FungibleConversionProxy {
    /// Main function for this contract, transfers fungible tokens to a payment address (to) with a payment reference, as well as a fee.
    /// The `amount` is denominated in `currency` with 2 decimals.
    ///
    /// Due to the way NEAR defines the fungible token standard, this function should NOT be called directly. Instead, the
    /// `ft_transfer_call` function in the contract of the fungible token being used for payment should be called with the
    /// `msg` argument containing the arguments for this function as a JSON-serialized string. `ft_on_transfer` (defined
    /// in this contract) is called by the token contract and deserializes the arguments and calls this function.
    ///
    /// See https://nomicon.io/Standards/Tokens/FungibleToken/Core for more information on how NEAR handles
    /// sending fungible tokens to be used by a contract function.
    ///
    /// # Arguments
    ///
    /// - `payment_reference`: used for indexing and matching the payment with a request
    /// - `token_address`: address of token used for payment
    /// - `to`: `amount` in `currency` of payment token will be paid to this address
    /// - `amount`: in `currency` with 2 decimals (eg. 1000 is 10.00)
    /// - `currency`: ticker, most likely fiat (eg. 'USD')
    /// - `fee_address`: `fee_amount` in `currency` of payment token will be paid to this address
    /// - `fee_amount`: in `currency`
    /// - `max_rate_timespan`: in nanoseconds, the maximum validity for the oracle rate response (or 0 if none)
    #[payable]
    fn transfer_with_reference(
        &mut self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payer: AccountId,
        deposit: U128,
    ) -> Promise {
        assert!(
            MIN_GAS <= env::prepaid_gas(),
            "Not enough attached Gas to call this method (Supplied: {}. Demand: {})",
            env::prepaid_gas(),
            MIN_GAS
        );

        let reference_vec: Vec<u8> = hex::decode(payment_reference.replace("0x", ""))
            .expect("Payment reference value error");
        assert_eq!(reference_vec.len(), 8, "Incorrect payment reference length");

        let callback_gas = BASIC_GAS * 3;

        // We need the token symbol and decimals for the oracle and conversion respectively
        fungible_token::ft_metadata(&token_address, NO_DEPOSIT, BASIC_GAS).then(
            ext_self::ft_metadata_callback(
                payment_reference,
                to,
                amount,
                currency,
                token_address,
                fee_address,
                fee_amount,
                max_rate_timespan,
                payer,
                deposit,
                &env::current_account_id(),
                env::attached_deposit(),
                callback_gas,
            ),
        )
    }

    /// Convenience function for getting arguments for `transfer_with_reference` in
    /// string format (json) so it can be included in the "msg" parameter of `ft_transfer_call`
    /// in fungible token contracts. Constructing the msg string could also easily be done on the
    /// frontend so is included here just for completeness and convenience.
    pub fn get_transfer_with_reference_args(
        &self,
        payment_reference: String,
        token_address: ValidAccountId,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
    ) -> String {
        let args = TransferWithReferenceArgs {
            payment_reference,
            token_address,
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            max_rate_timespan,
        };
        serde_json::to_string(&args).unwrap()
    }

    #[init]
    pub fn new(oracle_account_id: AccountId, provider_account_id: AccountId) -> Self {
        let owner_id = env::signer_account_id();
        Self {
            oracle_account_id,
            provider_account_id,
            owner_id,
        }
    }

    pub fn set_oracle_account(&mut self, oracle: ValidAccountId) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.oracle_account_id = oracle.to_string();
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn get_oracle_account(&self) -> AccountId {
        return self.oracle_account_id.to_string();
    }

    pub fn set_provider_account(&mut self, oracle: ValidAccountId) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.provider_account_id = oracle.to_string();
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn get_provider_account(&self) -> AccountId {
        return self.provider_account_id.to_string();
    }

    pub fn set_owner(&mut self, owner: ValidAccountId) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.owner_id = owner.to_string();
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    #[private]
    pub fn on_transfer_with_reference(
        &self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        fee_address: ValidAccountId,
        fee_amount: U128,
        crypto_amount: U128,
        crypto_fee_amount: U128,
        max_rate_timespan: U64,
        deposit: U128,
        change: U128,
        predecessor_account_id: AccountId,
        token_address: ValidAccountId,
    ) -> bool {
        near_sdk::assert_self();

        if near_sdk::is_promise_success() {
            fungible_token::ft_transfer(
                predecessor_account_id,
                change.0.to_string(),
                None,
                &token_address,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            );

            // Log success for indexing and payment detection
            env::log(
                &json!({
                    "amount": amount,
                    "currency": currency,
                    "token_address": token_address,
                    "fee_address": fee_address,
                    "fee_amount": fee_amount,
                    "max_rate_timespan": max_rate_timespan,
                    "payment_reference": payment_reference,
                    "to": to,
                    "crypto_amount": crypto_amount,
                    "crypto_fee_amount": crypto_fee_amount
                })
                .to_string()
                .into_bytes(),
            );
            true
        } else {
            env::log(
                format!(
                    "Failed to transfer to account {}. Returning attached deposit of {} of token {} to {}",
                    to, deposit.0, token_address, predecessor_account_id
                )
                .as_bytes(),
            );
            fungible_token::ft_transfer(
                predecessor_account_id,
                amount.0.to_string(),
                None,
                &token_address,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            );
            false
        }
    }

    #[private]
    #[payable]
    pub fn ft_metadata_callback(
        &mut self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payer: AccountId,
        deposit: U128,
    ) -> Promise {
        near_sdk::assert_self();

        // Parse fungible token metadata from promise result
        let ft_metadata = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                match serde_json::from_slice::<FungibleTokenMetadata>(&value) {
                    Ok(value) => value,
                    Err(_e) => panic!("ERR_INVALID_FT_METADATA_RESPONSE"),
                }
            }
            PromiseResult::Failed => panic!("ERR_FAILED_FT_METADATA_FETCH"),
        };

        let get_rate = fpo_contract::get_entry(
            ft_metadata.symbol + "/" + &currency,
            self.provider_account_id.clone(),
            &self.oracle_account_id,
            NO_DEPOSIT,
            BASIC_GAS,
        );
        let callback_gas = BASIC_GAS * 3;
        let process_request_payment = ext_self::rate_callback(
            payment_reference,
            to,
            amount,
            currency,
            token_address,
            fee_address,
            fee_amount,
            max_rate_timespan,
            payer,
            deposit,
            ft_metadata.decimals,
            &env::current_account_id(),
            env::attached_deposit(),
            callback_gas,
        );
        get_rate.then(process_request_payment)
    }

    #[private]
    #[payable]
    pub fn rate_callback(
        &mut self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payer: AccountId,
        deposit: U128,
        payment_token_decimals: u8,
    ) -> u128 {
        near_sdk::assert_self();

        // Parse rate from oracle promise result
        let rate = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                match serde_json::from_slice::<PriceEntry>(&value) {
                    Ok(value) => value,
                    Err(_e) => panic!("ERR_INVALID_ORACLE_RESPONSE"),
                }
            }
            PromiseResult::Failed => panic!("ERR_FAILED_ORACLE_FETCH"),
        };
        // Check rate validity
        assert!(
            u64::from(max_rate_timespan) == 0
                || rate.last_update >= env::block_timestamp() - u64::from(max_rate_timespan),
            "Conversion rate too old (Last updated: {})",
            rate.last_update,
        );
        let conversion_rate = u128::from(rate.price);
        let decimals = u32::from(rate.decimals); // this is the conversion rate decimals, not the token decimals
        let main_payment = Balance::from(amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;
        let fee_payment = Balance::from(fee_amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;

        let total_payment = main_payment + fee_payment;

        let main_payment_args =
            json!({ "receiver_id": to.to_string(), "amount":main_payment.to_string(), "memo": None::<String> })
                .to_string()
                .into_bytes();

        let fee_payment_args =
            json!({ "receiver_id": fee_address.to_string(), "amount":fee_payment.to_string(), "memo": None::<String> })
            .to_string()
            .into_bytes();

        Promise::new(token_address.to_string())
            .function_call(
                "ft_transfer".into(),
                main_payment_args,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            )
            .function_call(
                "ft_transfer".into(),
                fee_payment_args,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            )
            .then(ext_self::on_transfer_with_reference(
                payment_reference,
                to,
                amount,
                currency,
                fee_address,
                fee_amount,
                main_payment.into(),
                fee_payment.into(),
                max_rate_timespan,
                deposit,
                U128::from(deposit.0 - total_payment),
                payer,
                token_address,
                &env::current_account_id(),
                NO_DEPOSIT,
                BASIC_GAS,
            ));

        total_payment
    }
}
