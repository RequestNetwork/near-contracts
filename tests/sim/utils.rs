use mocks::fungible_token_mock::FungibleTokenContractContract;
use near_sdk::json_types::U128;
use near_sdk_sim::transaction::ExecutionStatus;
use near_sdk_sim::{call, to_yocto, ContractAccount, ExecutionResult, UserAccount};

pub fn assert_almost_eq_with_max_delta(left: u128, right: u128, max_delta: u128) {
    assert!(
        std::cmp::max(left, right) - std::cmp::min(left, right) <= max_delta,
        "{}",
        format!("Left {left} is not even close to Right {right} within delta {max_delta}")
    );
}

pub fn assert_eq_with_gas(left: u128, right: u128) {
    assert_almost_eq_with_max_delta(left, right, to_yocto("0.005"));
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
        "Spent {} instead of {}",
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

pub fn assert_one_promise_error(promise_result: ExecutionResult, expected_error_message: &str) {
    assert_eq!(promise_result.promise_errors().len(), 1);

    if let ExecutionStatus::Failure(execution_error) = &promise_result
        .promise_errors()
        .remove(0)
        .unwrap()
        .outcome()
        .status
    {
        assert!(
            execution_error.to_string().contains(expected_error_message),
            "Expected error containing: '{}'. Got: '{}'",
            expected_error_message,
            execution_error.to_string()
        );
    } else {
        unreachable!();
    }
}
