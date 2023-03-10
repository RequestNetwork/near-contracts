use mocks::fungible_token_mock::{FungibleTokenContract, FungibleTokenContractContract};
use near_sdk::json_types::U128;
use near_sdk_sim::errors::TxExecutionError;
use near_sdk_sim::transaction::ExecutionStatus;
use near_sdk_sim::{lazy_static_include, to_yocto, ExecutionResult, ContractAccount, call, UserAccount};

lazy_static_include::lazy_static_include_bytes! {
   REQUEST_PROXY_BYTES => "./out/conversion_proxy.wasm"
}

pub fn assert_almost_eq_with_max_delta(left: u128, right: u128, max_delta: u128) {
    assert!(
        std::cmp::max(left, right) - std::cmp::min(left, right) <= max_delta,
        "{}",
        format!(
            "Left {} is not even close to Right {} within delta {}",
            left, right, max_delta
        )
    );
}

pub fn assert_eq_with_gas(left: u128, right: u128) {
    assert_almost_eq_with_max_delta(left, right, to_yocto("0.005"));
}

/// Util to check a balance is the same as in a previous state
pub fn assert_unchanged_balance(account: UserAccount, previous_balance: u128, ft_contract: &ContractAccount<FungibleTokenContractContract>, account_name: &str) {
    
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(current_balance == previous_balance, "{}'s balance changed by {} (from {} to {})", account_name, previous_balance-current_balance, previous_balance, current_balance);
}

/// Util to TODO
pub fn assert_spent(account: UserAccount, previous_balance: u128, expected_spent_amount: u128, ft_contract: &ContractAccount<FungibleTokenContractContract>) {
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(current_balance <= previous_balance, "Did not spend.");
    assert!(current_balance == previous_balance -expected_spent_amount, "Spent {} instead of {}", previous_balance - current_balance, expected_spent_amount);
}

/// Util to TODO
pub fn assert_received(account: UserAccount, previous_balance: u128, expected_received_amount: u128, ft_contract: &ContractAccount<FungibleTokenContractContract>) {
    let current_balance = call!(account, ft_contract.ft_balance_of(account.account_id()))
        .unwrap_json::<U128>()
        .0;
    assert!(current_balance >= previous_balance, "Did not receive.");
    assert!(current_balance == previous_balance + expected_received_amount, "Received {} instead of {}", current_balance - previous_balance, expected_received_amount);
}

fn assert_error_message(execution_error: &TxExecutionError, expected_error_message: &str) {
    assert!(execution_error.to_string().contains(expected_error_message), "Got error '{}' instead of '{}'", execution_error.to_string(), expected_error_message);
}

pub fn assert_one_promise_error(promise_result: ExecutionResult, expected_error_message: &str) {
    assert_eq!(promise_result.promise_errors().len(), 1, "Got {} promise errors instead of 1", promise_result.promise_errors().len());

    if let ExecutionStatus::Failure(execution_error) = &promise_result
        .promise_errors()
        .remove(0)
        .unwrap()
        .outcome()
        .status
    {
        assert_error_message(execution_error, expected_error_message);
    } else {
        unreachable!();
    }
}

/// Util to test the error message for the latest failing promise
pub fn assert_last_promise_error(promise_result: ExecutionResult, expected_error_message: &str) {
    assert_ne!(promise_result.promise_errors().len(), 0, "The promise did not fail");

    if let ExecutionStatus::Failure(execution_error) = &promise_result
        .promise_errors()
        .remove(promise_result.promise_errors().len() - 1)
        .unwrap()
        .outcome()
        .status
    {
        assert_error_message(execution_error, expected_error_message);
    } else {
        unreachable!();
    }
}