use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    env, near_bindgen, serde_json, AccountId, Balance, Gas, Promise, PromiseResult, Timestamp, log,
};

near_sdk::setup_alloc!();

const NO_DEPOSIT: Balance = 0;
const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000;
// Fiat values with two decimals
const ONE_FIAT: Balance = 100;
const MIN_GAS: Gas = 50_000_000_000_000;
const BASIC_GAS: Gas = 10_000_000_000_000;

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
pub struct ConversionProxy {
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
    /// - `currency`: ticker, most likely fiat (eg. 'USD')
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

        let reference_vec: Vec<u8> = hex::decode(payment_reference.replace("0x", ""))
            .expect("Payment reference value error");
        assert_eq!(reference_vec.len(), 8, "Incorrect payment reference length");

        let get_rate = fpo_contract::get_entry(
            "NEAR/".to_owned() + &currency,
            self.provider_account_id.clone(),
            &self.oracle_account_id,
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
                payment_address, deposit.0, predecessor_account_id
            );
            Promise::new(predecessor_account_id).transfer(deposit.into());
            false
        }
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
        let decimals = u32::from(rate.decimals);
        let main_payment =
            Balance::from(amount) * ONE_NEAR * 10u128.pow(decimals) / conversion_rate / ONE_FIAT;
        let fee_payment = Balance::from(fee_amount) * ONE_NEAR * 10u128.pow(decimals)
            / conversion_rate
            / ONE_FIAT;

        let total_payment = main_payment + fee_payment;
        // Check deposit
        assert!(
            total_payment <= env::attached_deposit(),
            "Deposit too small for payment (Supplied: {}. Demand (incl. fees): {})",
            env::attached_deposit(),
            total_payment
        );

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

    fn default_values() -> (ValidAccountId, U128, ValidAccountId, U128, U64) {
        (
            alice_account().try_into().unwrap(),
            U128::from(12),
            bob_account().try_into().unwrap(),
            U128::from(1),
            U64::from(0),
        )
    }

    #[test]
    #[should_panic(expected = r#"Incorrect payment reference length"#)]
    fn transfer_with_invalid_reference_length() {
        let context = get_context(alice_account(), ntoy(100), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x11223344556677".to_string();
        let currency = "USD".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"Payment reference value error"#)]
    fn transfer_with_invalid_reference_value() {
        let context = get_context(alice_account(), ntoy(100), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x123".to_string();
        let currency = "USD".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
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
        let context = get_context(alice_account(), ntoy(1), 10u64.pow(13), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x1122334455667788".to_string();
        let currency = "USD".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    fn transfer_with_reference() {
        let context = get_context(alice_account(), ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let payment_reference = "0x1122334455667788".to_string();
        let currency = "USD".to_string();
        let (to, amount, fee_address, fee_amount, max_rate_timespan) = default_values();
        contract.transfer_with_reference(
            payment_reference,
            to,
            amount,
            currency,
            fee_address,
            fee_amount,
            max_rate_timespan,
        );
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_oracle_no_permission() {
        let context = get_context(alice_account(), ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_oracle_account(to);
    }

    #[test]
    fn admin_oracle() {
        let owner = ConversionProxy::default().owner_id;
        let mut contract = ConversionProxy::default();
        let context = get_context(owner, ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_oracle_account(to);
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_provider_no_permission() {
        let context = get_context(alice_account(), ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_provider_account(to);
    }

    #[test]
    fn admin_provider() {
        let owner = ConversionProxy::default().owner_id;
        let mut contract = ConversionProxy::default();
        let context = get_context(owner, ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_provider_account(to);
    }

    #[test]
    #[should_panic(expected = r#"ERR_PERMISSION"#)]
    fn admin_owner_no_permission() {
        let context = get_context(alice_account(), ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = ConversionProxy::default();
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_owner(to);
    }

    #[test]
    fn admin_owner() {
        let owner = ConversionProxy::default().owner_id;
        let mut contract = ConversionProxy::default();
        let context = get_context(owner, ntoy(1), 10u64.pow(14), false);
        testing_env!(context);
        let (to, _amount, _fee_address, _fee_amount, _max_rate_timespan) = default_values();
        contract.set_owner(to);
    }
}
