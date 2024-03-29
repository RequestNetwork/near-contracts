use std::convert::TryInto;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    bs58, env, log, near_bindgen, serde_json, AccountId, Balance, Gas, Promise, PromiseResult,
    PublicKey, Timestamp,
};

near_sdk::setup_alloc!();

const NO_DEPOSIT: Balance = 0;
const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000;
// Fiat values with two decimals
const ONE_FIAT: Balance = 100;
const MIN_GAS: Gas = 50_000_000_000_000;
const BASIC_GAS: Gas = 10_000_000_000_000;

/**
 * Switchboard oracle-related declarations
 */

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct SwitchboardDecimal {
    pub mantissa: i128,
    pub scale: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct PriceEntry {
    pub result: SwitchboardDecimal,
    pub num_success: u32,
    pub num_error: u32,
    pub round_open_timestamp: Timestamp,
}

pub type Uuid = [u8; 32];

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct SwitchboardIx {
    pub address: Uuid, // This feed address reference a specific price feed, see https://app.switchboard.xyz
    pub payer: Uuid,
}

// Interface of the Switchboard feed parser
#[near_sdk::ext_contract(sb_contract)]
trait Switchboard {
    fn aggregator_read(ix: SwitchboardIx) -> Promise<PriceEntry>;
}

///
/// This contract
/// - feed_parser: should be a valid Switchboard feed parser
/// - feed_address: should be a valid NEAR/USD price feed
/// - feed_payer: pays for feeds not sponsored by Switchboard
/// - owner_id: only the owner can edit the contract state values above (default = deployer)
#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct ConversionProxy {
    pub feed_parser: AccountId,
    pub feed_address: Uuid,
    pub feed_payer: Uuid,
    pub owner_id: AccountId,
}

// Callback methods
#[near_sdk::ext_contract(ext_self)]
pub trait ExtSelfRequestProxy {
    fn on_transfer_with_reference(
        &self,
        payment_reference: String,
        payment_address: ValidAccountId,
        amount: U128,
        currency: String,
        fee_payment_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        deposit: U128,
        change: U128,
        predecessor_account_id: AccountId,
    ) -> bool;

    fn rate_callback(
        &self,
        payment_address: ValidAccountId,
        amount: U128,
        currency: String,
        fee_payment_address: ValidAccountId,
        fee_amount: U128,
        payment_reference: String,
        max_rate_timespan: U64,
        payer: AccountId,
    ) -> u128;
}

#[near_bindgen]
impl ConversionProxy {
    /// Main external function for this contract,  transfers NEAR tokens to a payment address (to) with a payment reference, as well as a fee.
    /// The `amount` is denominated in `currency` with 2 decimals.
    ///
    /// # Arguments
    ///
    /// - `payment_reference`: used for indexing and matching the payment with a request
    /// - `payment_address`: `amount` in `currency` of NEAR will be paid to this address
    /// - `amount`: in `currency` with 2 decimals (eg. 1000 is 10.00)
    /// - `currency`: ticker, only "USD" implemented for now
    /// - `fee_payment_address`: `fee_amount` in `currency` of NEAR will be paid to this address
    /// - `fee_amount`: in `currency`
    /// - `max_rate_timespan`: in nanoseconds, the maximum validity for the oracle rate response (or 0 if none)
    #[payable]
    pub fn transfer_with_reference(
        &mut self,
        payment_reference: String,
        to: ValidAccountId,
        amount: U128,
        currency: String,
        fee_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
    ) -> Promise {
        assert!(
            MIN_GAS <= env::prepaid_gas(),
            "Not enough attached Gas to call this method (Supplied: {}. Demand: {})",
            env::prepaid_gas(),
            MIN_GAS
        );
        assert_eq!(
            currency, "USD",
            "Only payments denominated in USD are implemented for now"
        );

        let reference_vec: Vec<u8> = hex::decode(payment_reference.replace("0x", ""))
            .expect("Payment reference value error");
        assert_eq!(reference_vec.len(), 8, "Incorrect payment reference length");

        let get_rate = sb_contract::aggregator_read(
            SwitchboardIx {
                address: self.feed_address.clone(),
                payer: self.feed_payer.clone(),
            },
            &self.feed_parser,
            NO_DEPOSIT,
            BASIC_GAS,
        );
        let callback_gas = BASIC_GAS * 3;
        let process_request_payment = ext_self::rate_callback(
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            payment_reference,
            max_rate_timespan,
            env::predecessor_account_id(),
            &env::current_account_id(),
            env::attached_deposit(),
            callback_gas,
        );
        get_rate.then(process_request_payment)
    }

    #[init]
    pub fn new(feed_parser: AccountId, feed_address_pk: &String) -> Self {
        let owner_id = env::signer_account_id();
        let feed_payer = Self::get_uuid(env::signer_account_pk()).expect("ERR_OWNER_PK_LENGTH");
        let feed_address = Self::get_uuid_from_string(feed_address_pk);
        Self {
            feed_parser,
            feed_address,
            feed_payer,
            owner_id,
        }
    }

    pub fn set_feed_parser(&mut self, feed_parser: AccountId) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.feed_parser = feed_parser;
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn get_feed_parser(&self) -> AccountId {
        return self.feed_parser.clone();
    }

    pub fn set_feed_address(&mut self, feed_address: &String) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.feed_address = Self::get_uuid_from_string(feed_address);
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn get_feed_address(&self) -> Uuid {
        return self.feed_address.clone();
    }

    pub fn get_encoded_feed_address(&self) -> String {
        return bs58::encode(self.feed_address.clone()).into_string();
    }

    pub fn set_owner(&mut self, owner: ValidAccountId) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.owner_id = owner.to_string();
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn set_feed_payer(&mut self) {
        let signer_id = env::predecessor_account_id();
        if self.owner_id == signer_id {
            self.feed_payer =
                Self::get_uuid(env::signer_account_pk()).expect("ERR_OWNER_PK_LENGTH");
        } else {
            panic!("ERR_PERMISSION");
        }
    }

    pub fn get_feed_payer(&self) -> Uuid {
        return self.feed_payer.clone();
    }

    pub fn get_encoded_feed_payer(&self) -> String {
        return bs58::encode(self.feed_payer.clone()).into_string();
    }

    /// This method transforms a PublicKey (eg. ed25519:3H8UcosBhKfPcuZj7ffr3QqG5BxiGzJECqPZAZka5fJn) into a Uuid (alias for [u8; 32])
    /// Should be useless onchain.
    #[private]
    pub fn get_uuid(public_key: PublicKey) -> Option<Uuid> {
        let vec_length = public_key.len();
        if vec_length == 32 {
            return Some(public_key.try_into().unwrap());
        }
        // For some reason, the local VM sometimes prepends a 0 in front of the 32-long vector
        if vec_length == 33 && public_key[0] == 0_u8 {
            return Some(public_key[1..].try_into().unwrap());
        }
        return None;
    }

    #[private]
    pub fn get_uuid_from_string(public_key: &String) -> Uuid {
        bs58::decode(public_key)
            .into_vec()
            .expect("public_key should be decodable into a vector")
            .try_into()
            .expect("public_key should be decodable into [u8; 32]")
    }

    #[private]
    pub fn on_transfer_with_reference(
        &self,
        payment_reference: String,
        payment_address: ValidAccountId,
        amount: U128,
        currency: String,
        fee_payment_address: ValidAccountId,
        fee_amount: U128,
        max_rate_timespan: U64,
        deposit: U128,
        change: U128,
        predecessor_account_id: AccountId,
    ) -> bool {
        near_sdk::assert_self();

        if near_sdk::is_promise_success() {
            Promise::new(predecessor_account_id).transfer(change.into());

            // Log success for indexing and payment detection
            env::log(
                &json!({
                    "to": payment_address,
                    "amount": amount,
                    "currency": currency,
                    "payment_reference": payment_reference,
                    "fee_amount": fee_amount,
                    "fee_address": fee_payment_address,
                    "max_rate_timespan": max_rate_timespan,
                })
                .to_string()
                .into_bytes(),
            );
            true
        } else {
            log!(
                "Failed to transfer to account {}. Returning attached deposit of {} to {}",
                payment_address,
                deposit.0,
                predecessor_account_id
            );
            Promise::new(predecessor_account_id).transfer(deposit.into());
            false
        }
    }

    /// This method refunds a payer, then logs an error message.
    /// Used as a bandaid until we find a solution for refund.then(panic)
    #[private]
    pub fn refund_then_log(&mut self, payer: ValidAccountId, error_message: String) -> u128 {
        Promise::new(payer.clone().to_string()).transfer(env::attached_deposit());
        log!(error_message);
        return 0_u128;
    }

    /// Recursive util to convert an `amount` with a `converion_rate` having `decimals`.
    /// The `precision` is used to work around overflows, a precision of 10^n means that the result has a precision of n yocto digits.
    /// Said another way, a precision of 1000 will give a result rounded to the closest 1000 yoctos, ending with "...000" in yocto.
    #[private]
    pub fn apply_conversion_with_precision(
        amount: U128,
        decimals: u32,
        conversion_rate: u128,
        precision: u128,
    ) -> u128 {
        let (main_payment, flag) = (Balance::from(amount) * ONE_NEAR / ONE_FIAT / precision)
            .overflowing_mul(10u128.pow(u32::from(decimals)));
        if flag {
            return Self::apply_conversion_with_precision(
                amount,
                decimals,
                conversion_rate,
                precision * 10,
            );
        }
        let main_payment = (main_payment / conversion_rate) * precision;
        return main_payment;
    }

    #[private]
    pub fn apply_conversion(amount: U128, decimals: u32, conversion_rate: u128) -> u128 {
        return Self::apply_conversion_with_precision(amount, decimals, conversion_rate, 1);
    }

    #[private]
    #[payable]
    pub fn rate_callback(
        &mut self,
        payment_address: ValidAccountId,
        amount: U128,
        currency: String,
        fee_payment_address: ValidAccountId,
        fee_amount: U128,
        payment_reference: String,
        max_rate_timespan: U64,
        payer: ValidAccountId,
    ) -> u128 {
        near_sdk::assert_self();
        // Parse rate from oracle promise result
        let rate = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                match serde_json::from_slice::<PriceEntry>(&value) {
                    Ok(value) => value,
                    Err(_e) => {
                        return self.refund_then_log(payer, "ERR_INVALID_ORACLE_RESPONSE".into())
                    }
                }
            }
            PromiseResult::Failed => {
                return self.refund_then_log(payer, "ERR_FAILED_ORACLE_FETCH".into());
            }
        };
        // Check rate errors
        if rate.num_error != 0 || rate.num_success < 1 {
            return self.refund_then_log(
                payer,
                "Conversion errors:".to_string()
                    + &rate.num_error.to_string()
                    + &", successes: "
                    + &rate.num_success.to_string(),
            );
        }
        // Check rate validity
        if u64::from(max_rate_timespan) != 0
            && rate.round_open_timestamp < env::block_timestamp() - u64::from(max_rate_timespan)
        {
            return self.refund_then_log(
                payer,
                "Conversion rate too old (Last updated: ".to_string()
                    + &rate.round_open_timestamp.to_string()
                    + &")",
            );
        }
        let conversion_rate = 0_u128
            .checked_add_signed(rate.result.mantissa)
            .expect("The conversion rate should be positive");
        let main_payment = Self::apply_conversion(amount, rate.result.scale, conversion_rate);
        let fee_payment = Self::apply_conversion(fee_amount, rate.result.scale, conversion_rate);

        let total_payment = main_payment + fee_payment;
        // Check deposit
        if total_payment > env::attached_deposit() {
            return self.refund_then_log(
                payer,
                "Deposit too small for payment. Supplied: ".to_string()
                    + &env::attached_deposit().to_string()
                    + &". Demand (incl. fees): "
                    + &total_payment.to_string(),
            );
        }

        let change = env::attached_deposit() - (total_payment);

        // Make payment, log details and give change back
        Promise::new(payment_address.clone().to_string())
            .transfer(main_payment)
            .then(
                // Pay fees and declare payment
                Promise::new(fee_payment_address.to_string().clone())
                    .transfer(fee_payment)
                    .then(ext_self::on_transfer_with_reference(
                        payment_reference,
                        payment_address.clone(),
                        amount.into(),
                        currency,
                        fee_payment_address.clone(),
                        fee_amount.into(),
                        max_rate_timespan.into(),
                        U128::from(env::attached_deposit()),
                        U128::from(change),
                        payer.to_string(),
                        &env::current_account_id(),
                        NO_DEPOSIT,
                        BASIC_GAS,
                    )),
            );

        // result in NEAR with two decimals
        total_payment * 100 / ONE_NEAR
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

    fn bob_account() -> AccountId {
        "bob.near".to_string()
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
            signer_account_pk: (1..33).collect(), // Public key: Size 32
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

    fn default_values() -> (ValidAccountId, U128, ValidAccountId, U128, U64) {
        (
            alice_account().try_into().unwrap(),
            U128::from(12),
            bob_account().try_into().unwrap(),
            U128::from(1),
            U64::from(0),
        )
    }

    pub(crate) const USD: &str = "USD";
    pub(crate) const PAYMENT_REF: &str = "0x1122334455667788";
    pub(crate) const FEED_ADDRESS: &str = "HeS3xrDqHA2CSHTmN9osstz8vbXfgh2mzzzzzzzzzzzz";

    #[test]
    #[should_panic(expected = r#"Incorrect payment reference length"#)]
    fn transfer_with_invalid_reference_length() {
        testing_env!(get_context(
            alice_account(),
            ntoy(100),
            10u64.pow(14),
            false
        ));
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x11223344556677".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
            to,
            amount,
            USD.into(),
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"Payment reference value error"#)]
    fn transfer_with_invalid_reference_value() {
        testing_env!(get_context(
            alice_account(),
            ntoy(100),
            10u64.pow(14),
            false
        ));
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x123".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
            to,
            amount,
            USD.into(),
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"Only payments denominated in USD are implemented for now"#)]
    fn transfer_with_invalid_currency() {
        testing_env!(get_context(
            alice_account(),
            ntoy(100),
            10u64.pow(14),
            false
        ));
        let mut contract = ConversionProxy::default();
        let currency = "HKD".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            PAYMENT_REF.into(),
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"Not enough attached Gas to call this method"#)]
    fn transfer_with_not_enough_gas() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(13), false));
        let mut contract = ConversionProxy::default();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            PAYMENT_REF.into(),
            to,
            amount,
            USD.into(),
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    fn transfer_with_reference() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            PAYMENT_REF.into(),
            to,
            amount,
            USD.into(),
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_feed_address_no_permission() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        contract.set_feed_address(&FEED_ADDRESS.into());
    }

    #[test]
    fn admin_feed_address() {
        let owner = ConversionProxy::default().owner_id;
        testing_env!(get_context(owner, ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        contract.set_feed_address(&FEED_ADDRESS.into());
        assert_eq!(
            contract.get_encoded_feed_address(),
            FEED_ADDRESS.to_string()
        );
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_feed_payer_no_permission() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        contract.set_feed_payer();
    }

    #[test]
    fn admin_feed_payer() {
        let owner = ConversionProxy::default().owner_id;
        testing_env!(get_context(owner, ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        contract.set_feed_payer();
        assert_eq!(contract.get_feed_payer().to_vec(), env::signer_account_pk());
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_feed_parser_no_permission() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        let (to, _, _, _, _) = default_values();
        contract.set_feed_parser(to.into());
    }

    #[test]
    fn admin_feed_parser() {
        let owner = ConversionProxy::default().owner_id;
        let mut contract = ConversionProxy::default();
        testing_env!(get_context(owner, ntoy(1), 10u64.pow(14), false));
        let (to, _, _, _, _) = default_values();
        contract.set_feed_parser(to.clone().into());
        assert_eq!(contract.get_feed_parser(), Into::<AccountId>::into(to));
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_owner_no_permission() {
        testing_env!(get_context(alice_account(), ntoy(1), 10u64.pow(14), false));
        let mut contract = ConversionProxy::default();
        let (to, _, _, _, _) = default_values();
        contract.set_owner(to);
    }

    #[test]
    fn admin_owner() {
        let owner = ConversionProxy::default().owner_id;
        let mut contract = ConversionProxy::default();
        testing_env!(get_context(owner, ntoy(1), 10u64.pow(14), false));
        let (to, _, _, _, _) = default_values();
        contract.set_owner(to.clone());
        testing_env!(get_context(to.into(), ntoy(1), 10u64.pow(14), false));
        assert!(contract.owner_id == env::signer_account_id());
        assert!(contract.get_feed_payer().to_vec() != env::signer_account_pk());
        contract.set_feed_payer();
        assert_eq!(contract.get_feed_payer().to_vec(), env::signer_account_pk());
    }
}
