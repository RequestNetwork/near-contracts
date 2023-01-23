use near_sdk::assert_one_yocto;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near_bindgen, AccountId};

// For mocks: state of a fungible token
#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct FungibleTokenContract {}

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

/**
 * Mocked fungible token contract for tests
 */

#[near_bindgen]
impl FungibleTokenContract {
    #[allow(unused_variables)]
    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        env::log(format!("ft_transfer OK").as_bytes());
        assert_one_yocto();
    }

    pub fn ft_metadata(&self) -> Option<FungibleTokenMetadata> {
        env::log(format!("ft_metadata OK").as_bytes());
        Some(FungibleTokenMetadata {
            spec: "ft-1.0.0".into(),
            name: "USD Coin".into(),
            symbol: "USDC.e".into(),
            icon: None,
            reference: None,
            reference_hash: None,
            decimals: 6,
        })
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
    #[should_panic]
    fn test_ft_transfer_no_yocto() {
        let context = get_context("alice.near".to_string(), 0, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.ft_transfer("bob.near".to_string(), 100.into(), None);
    }

    #[test]
    fn test_ft_transfer() {
        let context = get_context("alice.near".to_string(), 1, 10u64.pow(14), false);
        testing_env!(context);
        let mut contract = FungibleTokenContract::default();

        contract.ft_transfer("bob.near".to_string(), 100.into(), None);
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
