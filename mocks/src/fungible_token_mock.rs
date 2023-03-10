use near_sdk::assert_one_yocto;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near_bindgen, AccountId};
use std::collections::HashMap;

/**
 * Mocking a fungible token contract (NEP-141)
 */

// Return type of a fungible token metadata
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

// For mocks: state of a fungible token
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, Serialize)]
pub struct FungibleTokenContract {
    symbol: String,
    balances: HashMap<AccountId, U128>,
}

/**
 * Mocked fungible token contract for tests
 */

#[near_bindgen]
impl FungibleTokenContract {
    /// Simulates a fungible token transfer. Ensures that:
    /// - both the sender and receiver are registered with the token contract
    /// - exactly one yoctoNEAR is attached to the call
    /// - sender's balance is sufficient for the transfer
    #[allow(unused_variables)]
    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: String, memo: Option<String>) {
        assert_one_yocto();
        assert!(
            self.balances.contains_key(&env::predecessor_account_id()),
            "sender is not registered with fungible token contract"
        );
        assert!(
            self.balances.contains_key(&receiver_id),
            "receiver is not registered with fungible token contract"
        );

        let amt = amount.parse::<u128>().unwrap();

        assert!(
            self.balances[&env::predecessor_account_id()].0 >= amt,
            "sender balance is insufficient"
        );

        let old_sender_balance = self.ft_balance_of(env::predecessor_account_id());
        let old_receiver_balance = self.ft_balance_of(receiver_id.to_string());

        self.set_balance(
            env::predecessor_account_id(),
            U128::from(old_sender_balance.0 - amt),
        );
        self.set_balance(receiver_id, U128::from(old_receiver_balance.0 + amt));

        env::log(format!("ft_transfer OK").as_bytes());
    }

    pub fn ft_metadata(&self) -> Option<FungibleTokenMetadata> {
        env::log(format!("ft_metadata OK").as_bytes());
        Some(FungibleTokenMetadata {
            spec: "ft-1.0.0".into(),
            name: "USD Coin".into(),
            symbol: self.symbol.to_owned(),
            icon: None,
            reference: None,
            reference_hash: None,
            decimals: 6,
        })
    }
    /// Helper function for testing
    pub fn set_symbol(&mut self, symbol: String) {
        self.symbol = symbol;
    }

    /// Helper function for testing
    pub fn set_balance(&mut self, account: AccountId, balance: U128) {
        *self.balances.get_mut(&account).unwrap() = balance;
    }

    /// Helper function for testing
    pub fn register_account(&mut self, account: AccountId) {
        self.balances.insert(account, 0.into());
    }

    /// Helper function for testing
    pub fn unregister_account(&mut self, account: AccountId) {
        self.balances.remove(&account);
    }

    /// Helper function for testing
    pub fn is_registered(&self, account: AccountId) -> bool {
        self.balances.contains_key(&account)
    }

    /// Helper function for testing
    pub fn ft_balance_of(&self, account: AccountId) -> U128 {
        assert!(
            self.balances.contains_key(&account),
            "account is not registered with fungible token contract"
        );
        self.balances[&account]
    }
}

impl Default for FungibleTokenContract {
    fn default() -> Self {
        Self { symbol: "USDC.e".into(), balances: HashMap::new() }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::fungible_token_mock::AccountId;
    use near_sdk::{testing_env, Balance, Gas, MockedBlockchain, VMContext};

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
            block_timestamp: 10,
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

    #[test]
    #[should_panic(expected = r#"Requires attached deposit of exactly 1 yoctoNEAR"#)]
    fn test_ft_transfer_no_yocto() {
        let context = get_context("alice.near".to_string(), 0, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.ft_transfer("bob.near".to_string(), "100".into(), None);
    }

    #[test]
    #[should_panic(expected = r#"sender balance is insufficient"#)]
    fn test_ft_transfer_sender_balance_too_low() {
        let context = get_context("alice.near".to_string(), 1, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.register_account("alice.near".to_string());
        contract.register_account("bob.near".to_string());
        contract.set_balance("alice.near".to_string(), 99.into());

        contract.ft_transfer("bob.near".to_string(), "100".into(), None);
    }

    #[test]
    #[should_panic(expected = r#"receiver is not registered with fungible token contract"#)]
    fn test_ft_transfer_receiver_not_registered() {
        let context = get_context("alice.near".to_string(), 1, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.register_account("alice.near".to_string());
        contract.set_balance("alice.near".to_string(), 100.into());

        contract.ft_transfer("bob.near".to_string(), "100".into(), None);
    }

    #[test]
    fn test_ft_transfer() {
        let context = get_context("alice.near".to_string(), 1, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.register_account("alice.near".to_string());
        contract.register_account("bob.near".to_string());
        contract.set_balance("alice.near".to_string(), 100.into());

        contract.ft_transfer("bob.near".to_string(), "100".into(), None);
    }

    #[test]
    fn test_ft_metadata() {
        let context = get_context("alice.near".to_string(), 0, 10u64.pow(14), true);
        testing_env!(context);
        let contract = FungibleTokenContract::default();

        if let Some(result) = contract.ft_metadata() {
            assert_eq!(result.symbol, "USDC.e");
            assert_eq!(result.decimals, 6);
        } else {
            panic!("Fungible token metadata mock returned None")
        }
    }
}
