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
const MIN_GAS: Gas = 150_000_000_000_000;
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
#[near_sdk::ext_contract(ft_contract)]
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
    ) -> String;

    fn ft_metadata_callback(
        &self,
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
    ) -> Promise;

    fn rate_callback(
        &self,
        args: PayerSuppliedArgs,
        payer: AccountId,
        deposit: U128,
        payment_token_decimals: u8,
    ) -> Promise;
}

trait FungibleTokenReceiver {
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: String, msg: String) -> Promise;
}

#[near_bindgen]
impl FungibleTokenReceiver for FungibleConversionProxy {
    /// This is the function that will be called by the fungible token contract's `ft_transfer_call` function.
    /// We use the `msg` field to obtain the arguments supplied by the caller specifying the intended payment.
    /// `msg` should be a string in JSON format containing all the fields in `PayerSuppliedArgs`.
    /// Eg. msg = {"payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near","amount":"1000000","currency":"USD","token_address":"a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near","fee_address":"fee.requestfinance.near","fee_amount":"200","max_rate_timespan":"0"}
    ///
    /// For more information on the fungible token standard, see https://nomicon.io/Standards/Tokens/FungibleToken/Core
    ///
    #[payable]
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: String, msg: String) -> Promise {
        let args: PayerSuppliedArgs = serde_json::from_str(&msg).expect("Incorrect msg format");
        self.transfer_with_reference(args, sender_id, U128::from(amount.parse::<u128>().unwrap()))
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
    #[private]
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
        ft_contract::ft_metadata(&args.token_address, NO_DEPOSIT, BASIC_GAS).then(
            ext_self::ft_metadata_callback(
                args,
                payer,
                deposit,
                &env::current_account_id(),
                env::attached_deposit(),
                BASIC_GAS * 12,
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
    ) -> String {
        if near_sdk::is_promise_success() {
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
            change.0.to_string() // return change for `ft_resolve_transfer` on the token contract
        } else {
            env::log(
                format!(
                    "Failed to transfer to account {}. Returning attached deposit of {} of token {} to {}",
                    args.to, deposit.0, args.token_address, payer)
                .as_bytes(),
            );
            deposit.0.to_string() // return full amount for `ft_resolve_transfer` on the token contract
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
            BASIC_GAS * 8,
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
    ) -> Promise {
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
        let amount = Balance::from(args.amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;
        let fee_amount = Balance::from(args.fee_amount)
            * 10u128.pow(payment_token_decimals.into())
            * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;

        let total_amount = amount + fee_amount;

        // Check deposit
        assert!(total_amount <= deposit.0, "Deposit too small");

        let change = deposit.0 - total_amount;

        let amount_args =
            json!({ "receiver_id": args.to.to_string(), "amount":amount.to_string(), "memo": None::<String> })
                .to_string()
                .into_bytes();

        let fee_amount_args =
            json!({ "receiver_id": args.fee_address.to_string(), "amount":fee_amount.to_string(), "memo": None::<String> })
            .to_string()
            .into_bytes();

        // Batch cross-contract calls (either both succeed or both fail)
        Promise::new(args.token_address.to_string())
            .function_call(
                "ft_transfer".into(),
                amount_args,
                YOCTO_DEPOSIT,
                BASIC_GAS * 2,
            )
            .function_call(
                "ft_transfer".into(),
                fee_amount_args,
                YOCTO_DEPOSIT,
                BASIC_GAS * 2,
            )
            .then(ext_self::on_transfer_with_reference(
                args,
                payer,
                deposit,
                U128::from(amount),
                U128::from(fee_amount),
                U128::from(change),
                &env::current_account_id(),
                NO_DEPOSIT,
                BASIC_GAS,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{testing_env, VMContext};
    use near_sdk::{AccountId, Balance, MockedBlockchain};
    use std::convert::TryInto;

    fn alice_account() -> AccountId {
        "alice.near".to_string()
    }

    fn get_context(
        predecessor_account_id: AccountId,
        attached_deposit: Balance,
        prepaid_gas: Gas,
        is_view: bool,
    ) -> VMContext {
        VMContext {
            current_account_id: predecessor_account_id.clone(),
            signer_account_id: predecessor_account_id.clone(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 1,
            block_timestamp: 0,
            epoch_height: 1,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 10u64.pow(6),
            attached_deposit,
            prepaid_gas,
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
        }
    }

    fn ntoy(near_amount: Balance) -> Balance {
        near_amount * 10u128.pow(24)
    }

    /// Helper function: get default values for PayerSuppliedArgs
    fn get_default_payer_supplied_args() -> PayerSuppliedArgs {
        PayerSuppliedArgs {
            amount: 1000000.into(),
            currency: "USD".into(),
            token_address: "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near"
                .to_string()
                .try_into()
                .unwrap(),
            fee_address: "fee.requestfinance.near".to_string().try_into().unwrap(),
            fee_amount: 200.into(),
            max_rate_timespan: 0.into(),
            payment_reference: "abc7c8bb1234fd12".into(),
            to: "dummy.payee.near".to_string().try_into().unwrap(),
        }
    }

    /// Helper function: convert a PayerSuppliedArgs into the msg string to be passed into `ft_transfer_call`
    fn get_msg_from_args(args: PayerSuppliedArgs) -> String {
        serde_json::to_string(&args).unwrap()
    }

    #[test]
    #[should_panic(expected = r#"Incorrect payment reference length"#)]
    fn transfer_with_invalid_reference_length() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let mut args = get_default_payer_supplied_args();
        args.payment_reference = "0x11223344556677".to_string();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Payment reference value error"#)]
    fn transfer_with_invalid_reference_value() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let mut args = get_default_payer_supplied_args();
        args.payment_reference = "0x123".to_string();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Not enough attached Gas to call this method"#)]
    fn transfer_with_not_enough_gas() {
        let context = get_context(alice_account(), ntoy(100), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let args = get_default_payer_supplied_args();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Incorrect msg format"#)]
    fn transfer_with_invalid_msg_format() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let args = get_default_payer_supplied_args();
        let msg = get_msg_from_args(args) + ".";

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Incorrect msg format"#)]
    fn transfer_with_msg_missing_fields() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let args = get_default_payer_supplied_args();
        let msg = get_msg_from_args(args).replace("\"amount\":\"1000000\",", "");

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }
    #[test]
    fn transfer_with_reference() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let args = get_default_payer_supplied_args();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    fn test_get_transfer_with_reference_args() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, true);
        testing_env!(context);
        let contract = FungibleConversionProxy::default();

        let expected_msg = r#"{"amount":"1000000","currency":"USD","token_address":"a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near","fee_address":"fee.requestfinance.near","fee_amount":"200","max_rate_timespan":"0","payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near"}"#;
        let args = get_default_payer_supplied_args();

        let msg = contract.get_transfer_with_reference_args(
            args.amount,
            args.currency,
            args.token_address,
            args.fee_address,
            args.fee_amount,
            args.max_rate_timespan,
            args.payment_reference,
            args.to,
        );
        assert_eq!(msg, expected_msg);
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_oracle_no_permission() {
        let context = get_context(alice_account(), ntoy(1), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let new_orcale: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_oracle_account(new_orcale);
    }

    #[test]
    fn admin_oracle() {
        let owner = FungibleConversionProxy::default().owner_id;
        let mut contract = FungibleConversionProxy::default();
        let context = get_context(owner, ntoy(1), MIN_GAS, false);
        testing_env!(context);

        let new_orcale: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_oracle_account(new_orcale.clone());
        assert_eq!(contract.oracle_account_id, new_orcale.to_string());
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_provider_no_permission() {
        let context = get_context(alice_account(), ntoy(1), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let new_provider: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_provider_account(new_provider);
    }

    #[test]
    fn admin_provider() {
        let owner = FungibleConversionProxy::default().owner_id;
        let mut contract = FungibleConversionProxy::default();
        let context = get_context(owner, ntoy(1), 10u64.pow(14), false);
        testing_env!(context);

        let new_provider: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_provider_account(new_provider.clone());
        assert_eq!(contract.provider_account_id, new_provider.to_string());
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_owner_no_permission() {
        let context = get_context(alice_account(), ntoy(1), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleConversionProxy::default();

        let new_owner: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_owner(new_owner);
    }

    #[test]
    fn admin_owner() {
        let owner = FungibleConversionProxy::default().owner_id;
        let mut contract = FungibleConversionProxy::default();
        let context = get_context(owner, ntoy(1), MIN_GAS, false);
        testing_env!(context);

        let new_owner: ValidAccountId = alice_account().try_into().unwrap();
        contract.set_owner(new_owner.clone());
        assert_eq!(contract.owner_id, new_owner.to_string());
    }
}
