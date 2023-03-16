use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    env, near_bindgen, serde_json, AccountId, Balance, Gas, Promise
};
near_sdk::setup_alloc!();

const NO_DEPOSIT: Balance = 0;
const YOCTO_DEPOSIT: Balance = 1; // Fungible token transfers require a deposit of exactly 1 yoctoNEAR
const MIN_GAS: Gas = 150_000_000_000_000;
const BASIC_GAS: Gas = 10_000_000_000_000;

/// Helper struct containing arguments supplied by the caller
///
/// - `amount`: in `currency` with 2 decimals (eg. 1000 is 10.00)
/// - `fee_address`: `fee_amount` in `currency` of payment token will be paid to this address
/// - `fee_amount`: in `currency`
/// - `payment_reference`: used for indexing and matching the payment with a request
/// - `to`: `amount` in `currency` of payment token will be paid to this address
#[derive(Serialize, Deserialize)]
pub struct PaymentArgs {
    pub fee_address: ValidAccountId,
    pub fee_amount: U128,
    pub payment_reference: String,
    pub to: ValidAccountId,
}

impl Into<PaymentArgs> for String {
    fn into(self) -> PaymentArgs {
        serde_json::from_str(&self).expect("Incorrect msg format")
    }
}

impl Into<String> for PaymentArgs {
    fn into(self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}

/**
 * Fungible token-related declarations
 */

// Interface of fungible tokens
#[near_sdk::ext_contract(ft_contract)]
trait FungibleTokenContract {
    fn ft_transfer(receiver_id: String, amount: String, memo: Option<String>);
}

///
/// This contract
#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct FungibleProxy {}

// Callback methods
#[near_sdk::ext_contract(ext_self)]
pub trait ExtSelfRequestProxy {
    fn on_transfer_with_reference(
        &self,
        args: PaymentArgs,
        token_address: AccountId,
        payer: AccountId,
        amount: U128,
    ) -> String;
}

trait FungibleTokenReceiver {
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: String, msg: String) -> Promise;
}

#[near_bindgen]
impl FungibleTokenReceiver for FungibleProxy {
    /// This is the function that will be called by the fungible token contract's `ft_transfer_call` function.
    /// We use the `msg` field to obtain the arguments supplied by the caller specifying the intended payment.
    /// `msg` should be a string in JSON format containing all the fields in `PaymentArgs`.
    /// Eg. msg = {"payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near","fee_address":"fee.requestfinance.near","fee_amount":"200"}
    ///
    /// For more information on the fungible token standard, see https://nomicon.io/Standards/Tokens/FungibleToken/Core
    ///
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: String, msg: String) -> Promise {
        let token_address = env::predecessor_account_id();
        self.transfer_with_reference(
            msg.into(),
            token_address,
            sender_id,
            U128::from(amount.parse::<u128>().unwrap()),
        )
    }
}

#[near_bindgen]
impl FungibleProxy {
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
    fn transfer_with_reference(
        &mut self,
        args: PaymentArgs,
        token_address: AccountId,
        payer: AccountId,
        amount: U128,
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
        assert!(args.fee_amount.0 <= amount.0, "amount smaller than fee_amount");
        let main_amount = amount.0 - args.fee_amount.0;
        let main_transfer_args =
            json!({ "receiver_id": args.to.to_string(), "amount":main_amount.to_string(), "memo": None::<String> })
                .to_string()
                .into_bytes();

        let fee_transfer_args =
            json!({ "receiver_id": args.fee_address.to_string(), "amount":args.fee_amount.0.to_string(), "memo": None::<String> })
            .to_string()
            .into_bytes();

        Promise::new(token_address.to_string())
            .function_call(
                "ft_transfer".into(),
                main_transfer_args,
                YOCTO_DEPOSIT,
                BASIC_GAS * 2,
            )
            .function_call(
                "ft_transfer".into(),
                fee_transfer_args,
                YOCTO_DEPOSIT,
                BASIC_GAS * 2,
            ).then(ext_self::on_transfer_with_reference(
                args,
                token_address,
                payer,
                main_amount.into(),
                &env::current_account_id(),
                NO_DEPOSIT,
                BASIC_GAS,
            )
        )
    }

    #[private]
    pub fn on_transfer_with_reference(
        &self,
        args: PaymentArgs,
        token_address: AccountId,
        payer: AccountId,
        amount: U128,
    ) -> String {
        if near_sdk::is_promise_success() {
            // Log success for indexing and payment detection
            env::log(
                &json!({
                    "amount": amount,
                    "token_address": token_address,
                    "fee_address": args.fee_address,
                    "fee_amount": args.fee_amount,
                    "payment_reference": args.payment_reference,
                    "to": args.to,
                })
                .to_string()
                .into_bytes(),
            );
            0.to_string()
        } else {
            env::log(
                format!(
                    "Failed to transfer to account {}. Returning attached amount of {} of token {} to {}",
                    args.to, amount.0, token_address, payer)
                .as_bytes(),
            );
            (amount.0 + args.fee_amount.0).to_string() // return full amount for `ft_resolve_transfer` on the token contract
        }
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

    /// Helper function: get default values for PaymentArgs
    fn get_default_payment_args() -> PaymentArgs {
        PaymentArgs {
            amount: 1000000.into(),
            fee_address: "fee.requestfinance.near".to_string().try_into().unwrap(),
            fee_amount: 200.into(),
            payment_reference: "abc7c8bb1234fd12".into(),
            to: "dummy.payee.near".to_string().try_into().unwrap(),
        }
    }

    /// Helper function: convert a PaymentArgs into the msg string to be passed into `ft_transfer_call`
    fn get_msg_from_args(args: PaymentArgs) -> String {
        serde_json::to_string(&args).unwrap()
    }

    #[test]
    #[should_panic(expected = r#"Incorrect payment reference length"#)]
    fn transfer_with_invalid_reference_length() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let mut args = get_default_payment_args();
        args.payment_reference = "0x11223344556677".to_string();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Payment reference value error"#)]
    fn transfer_with_invalid_reference_value() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let mut args = get_default_payment_args();
        args.payment_reference = "0x123".to_string();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Not enough attached Gas to call this method"#)]
    fn transfer_with_not_enough_gas() {
        let context = get_context(alice_account(), ntoy(100), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let args = get_default_payment_args();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Incorrect msg format"#)]
    fn transfer_with_invalid_msg_format() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let args = get_default_payment_args();
        let msg = get_msg_from_args(args) + ".";

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"Incorrect msg format"#)]
    fn transfer_with_msg_missing_fields() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let args = get_default_payment_args();
        let msg = get_msg_from_args(args).replace("\"amount\":\"1000000\",", "");

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    #[should_panic(expected = r#"amount smaller than fee_amount"#)]
    fn transfer_less_than_fee_amount() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let args = get_default_payment_args();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1".into(), msg);
    }

    #[test]
    fn transfer_with_reference() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, false);
        testing_env!(context);
        let mut contract = FungibleProxy::default();

        let args = get_default_payment_args();
        let msg = get_msg_from_args(args);

        contract.ft_on_transfer(alice_account(), "1000001".into(), msg);
    }

    #[test]
    fn test_get_transfer_with_reference_args() {
        let context = get_context(alice_account(), ntoy(100), MIN_GAS, true);
        testing_env!(context);

        let expected_msg = r#"{"amount":"1000000","fee_address":"fee.requestfinance.near","fee_amount":"200","payment_reference":"abc7c8bb1234fd12","to":"dummy.payee.near"}"#;
        let msg: String = get_default_payment_args().into();
        assert_eq!(msg, expected_msg);
    }
}
