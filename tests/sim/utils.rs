use mocks::fungible_token_mock::FungibleTokenContractContract;
use near_sdk::json_types::U128;
use near_sdk_sim::transaction::ExecutionStatus;
use near_sdk_sim::{call, ContractAccount, ExecutionResult, UserAccount};

/// Util to compare 2 numbers in yocto, +/- 1 yocto to ignore math precision issues
pub fn yocto_almost_eq(left: u128, right: u128) -> bool {
    return std::cmp::max(left, right) - std::cmp::min(left, right) <= 1;
}

/// Util to check a balance is the same as in a previous state
pub fn assert_unchanged_balance(
    account: UserAccount,
    previous_balance: u128,
    ft_contract: &ContractAccount<FungibleTokenContractContract>,
    account_name: &str,
) {
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(
        current_balance == previous_balance,
        "{}'s balance changed by {} (from {} to {})",
        account_name,
        previous_balance - current_balance,
        previous_balance,
        current_balance
    );
}

/// Util to assert that an account has spent a given amount of token.
pub fn assert_spent(
    account: UserAccount,
    previous_balance: u128,
    expected_spent_amount: u128,
    ft_contract: &ContractAccount<FungibleTokenContractContract>,
) {
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(current_balance <= previous_balance, "Did not spend.");
    assert!(
        current_balance == previous_balance - expected_spent_amount,
        "Spent      {}\ninstead of {}",
        previous_balance - current_balance,
        expected_spent_amount
    );
}

/// Util to assert that an account has received a given amount of token.
pub fn assert_received(
    account: UserAccount,
    previous_balance: u128,
    expected_received_amount: u128,
    ft_contract: &ContractAccount<FungibleTokenContractContract>,
) {
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(current_balance >= previous_balance, "Did not receive.");
    assert!(
        current_balance == previous_balance + expected_received_amount,
        "Received {} instead of {}",
        current_balance - previous_balance,
        expected_received_amount
    );
}

pub trait ExecutionResultAssertion {
    fn assert_one_promise_error(&self, expected_error: &str);
    fn assert_success_one_log(&self, expected_log: &str);
}

impl ExecutionResultAssertion for ExecutionResult {
    fn assert_one_promise_error(&self, expected_error: &str) {
        assert!(!self.is_ok(), "Promise succeeded, expected to fail.");
        assert_eq!(
            self.promise_errors().len(),
            1,
            "Expected 1 error, got {}",
            self.promise_errors().len()
        );

        if let ExecutionStatus::Failure(execution_error) =
            &self.promise_errors().remove(0).unwrap().outcome().status
        {
            assert!(
                execution_error.to_string().contains(expected_error),
                "Expected error containing: '{}'. Got: '{}'",
                expected_error,
                execution_error.to_string()
            );
        } else {
            unreachable!();
        }
    }

    fn assert_success_one_log(&self, expected_log: &str) {
        self.assert_success();
        assert_eq!(self.logs().len(), 1, "Wrong number of logs");
        assert!(self.logs()[0].contains(&expected_log));
    }
}
