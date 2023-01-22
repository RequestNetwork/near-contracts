use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{Base64VecU8, ValidAccountId, U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    env, near_bindgen, serde_json, AccountId, Balance, Gas, Promise, PromiseResult, Timestamp,
};
near_sdk::setup_alloc!();

const NO_DEPOSIT: Balance = 0;
const YOCTO_DEPOSIT: Balance = 1; // Fungible token transfers require a deposit of exactly 1 yoctoNEAR
const ONE_FIAT: Balance = 100; // Fiat values with two decimals
const MIN_GAS: Gas = 100_000_000_000_000;
const BASIC_GAS: Gas = 10_000_000_000_000;

/// Helper struct containing arguments supplied by the caller
///
/// - `amount`: in `currency` with 2 decimals (eg. 1000 is 10.00)
/// - `currency`: ticker, most likely fiat (eg. 'USD')
/// - `token_address`: address of token used for payment
/// - `fee_address`: `fee_amount` in `currency` of payment token will be paid to this address
/// - `fee_amount`: in `currency`
/// - `max_rate_timespan`: in nanoseconds, the maximum validity for the oracle rate response (or 0 if none)
/// - `payment_reference`: used for indexing and matching the payment with a request
/// - `to`: `amount` in `currency` of payment token will be paid to this address
#[derive(Serialize, Deserialize)]
pub struct PayerSuppliedArgs {
    amount: U128,
    currency: String,
    token_address: ValidAccountId,
    fee_address: ValidAccountId,
    fee_amount: U128,
    max_rate_timespan: U64,
    payment_reference: String,
    to: ValidAccountId,
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
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
        crypto_amount: U128,
        crypto_fee_amount: U128,
        change: U128,
    ) -> bool;

    fn ft_metadata_callback(
        &self,
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
    ) -> u128;

    fn rate_callback(
        &self,
        args: PayerSuppliedArgs,
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
    /// This is the function that will be called by the fungible token contract's `ft_transfer_call` function.
    /// We use the `msg` field to obtain the arguments supplied by the caller specifying the intended payment.
    /// `msg` should be a string in JSON format containing all the fields in `PayerSuppliedArgs`.
    /// Eg. msg = {"payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near","amount":1000000,"currency":"USD","token_address":"a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near","fee_address":"fee.requestfinance.near","fee_amount":200,"max_rate_timespan":0}
    ///
    /// For more information on the fungible token standard, see https://nomicon.io/Standards/Tokens/FungibleToken/Core
    ///
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> String {
        let args: PayerSuppliedArgs = serde_json::from_str(&msg).unwrap();
        self.transfer_with_reference(args, sender_id, amount);

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
    #[payable]
    fn transfer_with_reference(
        &mut self,
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
    ) -> Promise {
        assert!(
            MIN_GAS <= env::prepaid_gas(),
            "Not enough attached Gas to call this method (Supplied: {}. Demand: {})",
            env::prepaid_gas(),
            MIN_GAS
        );

        let reference_vec: Vec<u8> = hex::decode(args.payment_reference.replace("0x", ""))
            .expect("Payment reference value error");
        assert_eq!(reference_vec.len(), 8, "Incorrect payment reference length");

        // We need to get the token symbol and decimals for the oracle and currency conversion respectively
        fungible_token::ft_metadata(&args.token_address, NO_DEPOSIT, BASIC_GAS).then(
            ext_self::ft_metadata_callback(
                args,
                payer,
                deposit,
                &env::current_account_id(),
                env::attached_deposit(),
                BASIC_GAS * 8,
            ),
        )
    }

    /// Convenience function for constructing the `msg` argument for `ft_transfer_call` in the fungible token contract.
    /// Constructing the `msg` string could also easily be done on the frontend so is included here just for completeness
    /// and convenience.
    pub fn get_transfer_with_reference_args(
        &self,
        amount: U128,
        currency: String,
        token_address: ValidAccountId,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        payment_reference: String,
        to: ValidAccountId,
    ) -> String {
        let args = PayerSuppliedArgs {
            amount,
            currency,
            token_address,
            fee_address,
            fee_amount,
            max_rate_timespan,
            payment_reference,
            to,
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
        self.oracle_account_id.to_string()
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
        self.provider_account_id.to_string()
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
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
        crypto_amount: U128,
        crypto_fee_amount: U128,
        change: U128,
    ) -> bool {
        near_sdk::assert_self();

        if near_sdk::is_promise_success() {
            // return extra change to payer
            fungible_token::ft_transfer(
                payer,
                change.0.to_string(),
                None,
                &args.token_address,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            );

            // Log success for indexing and payment detection
            env::log(
                &json!({
                    "amount": args.amount,
                    "currency": args.currency,
                    "token_address": args.token_address,
                    "fee_address": args.fee_address,
                    "fee_amount": args.fee_amount,
                    "max_rate_timespan": args.max_rate_timespan,
                    "payment_reference": args.payment_reference,
                    "to": args.to,
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
                    args.to, deposit.0, args.token_address, payer)
                .as_bytes(),
            );
            // Return full amount to payer
            fungible_token::ft_transfer(
                payer,
                args.amount.0.to_string(),
                None,
                &args.token_address,
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
        args: PayerSuppliedArgs,
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
            ft_metadata.symbol + "/" + &args.currency,
            self.provider_account_id.clone(),
            &self.oracle_account_id,
            NO_DEPOSIT,
            BASIC_GAS,
        );
        let process_request_payment = ext_self::rate_callback(
            args,
            payer,
            deposit,
            ft_metadata.decimals,
            &env::current_account_id(),
            env::attached_deposit(),
            BASIC_GAS * 6,
        );
        get_rate.then(process_request_payment)
    }

    #[private]
    #[payable]
    pub fn rate_callback(
        &mut self,
        args: PayerSuppliedArgs,
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
            u64::from(args.max_rate_timespan) == 0
                || rate.last_update >= env::block_timestamp() - u64::from(args.max_rate_timespan),
            "Conversion rate too old (Last updated: {})",
            rate.last_update,
        );
        let conversion_rate = u128::from(rate.price);
        let decimals = u32::from(rate.decimals); // this is the conversion rate decimals, not the token decimals
        let crypto_amount = Balance::from(args.amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;
        let crypto_fee_amount = Balance::from(args.fee_amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;

        let total_amount = crypto_amount + crypto_fee_amount;

        // Check deposit
        assert!(
            total_amount <= deposit.0,
            "Deposit too small for payment (Supplied: {}. Demand (incl. fees): {})",
            deposit.0,
            total_amount
        );

        let change = deposit.0 - total_amount;

        let crypto_amount_args =
            json!({ "receiver_id": args.to.to_string(), "amount":crypto_amount.to_string(), "memo": None::<String> })
                .to_string()
                .into_bytes();

        let crypto_fee_amount_args =
            json!({ "receiver_id": args.fee_address.to_string(), "amount":crypto_fee_amount.to_string(), "memo": None::<String> })
            .to_string()
            .into_bytes();

        // Batch cross-contract calls (either both succeed or both fail)
        Promise::new(args.token_address.to_string())
            .function_call(
                "ft_transfer".into(),
                crypto_amount_args,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            )
            .function_call(
                "ft_transfer".into(),
                crypto_fee_amount_args,
                YOCTO_DEPOSIT,
                BASIC_GAS,
            )
            .then(ext_self::on_transfer_with_reference(
                args,
                payer,
                deposit,
                U128::from(crypto_amount),
                U128::from(crypto_fee_amount),
                U128::from(change),
                &env::current_account_id(),
                NO_DEPOSIT,
                BASIC_GAS * 2,
            ));

        change
    }
}
