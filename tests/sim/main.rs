use crate::utils::*;
use conversion_proxy::ConversionProxyContract;
use mocks::FPOContractContract;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use near_sdk_sim::init_simulator;
use near_sdk_sim::runtime::GenesisConfig;
use near_sdk_sim::ContractAccount;
use near_sdk_sim::UserAccount;
use near_sdk_sim::{call, deploy, lazy_static_include, to_yocto};
use std::convert::TryInto;

near_sdk::setup_alloc!();

const CONTRACT_ID: &str = "request_proxy";
lazy_static_include::lazy_static_include_bytes! {
   REQUEST_PROXY_BYTES => "out/conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   MOCKED_FPO_BYTES => "out/mocks.wasm"
}

mod utils;

fn init() -> (
    UserAccount,
    UserAccount,
    ContractAccount<ConversionProxyContract>,
    // TODO remove:
    ContractAccount<FPOContractContract>,
) {
    let genesis = GenesisConfig::default();
    let root = init_simulator(Some(genesis));

    let fpo: ContractAccount<FPOContractContract> = deploy!(
        contract: FPOContractContract,
        contract_id: "mockedfpo".to_string(),
        bytes: &MOCKED_FPO_BYTES,
        signer_account: root,
        deposit: to_yocto("5")
    );

    let account = root.create_user("alice".to_string(), to_yocto("1000"));

    let request_proxy = deploy!(
        contract: ConversionProxyContract,
        contract_id: CONTRACT_ID,
        bytes: &REQUEST_PROXY_BYTES,
        signer_account: root,
        deposit: to_yocto("5"),
        init_method: new("mockedfpo".into(), "any".into())
    );

    let get_oracle_result = call!(root, request_proxy.get_oracle_account());
    get_oracle_result.assert_success();

    debug_assert_eq!(
        &get_oracle_result.unwrap_json_value().to_owned(),
        &"mockedfpo".to_string()
    );

    (root, account, request_proxy, fpo)
}

#[test]
fn test_transfer_with_reference() {
    let initial_alice_balance = to_yocto("100");
    let initial_bob_balance = to_yocto("10");
    let transfer_amount = to_yocto("50");

    let (root, alice, request_proxy, mocked_fpo) = init();

    let bob = root.create_user("bob".parse().unwrap(), initial_bob_balance);
    // Transfer of tokens
    let result_mocked = call!(
        alice,
        mocked_fpo.get_entry("NEAR/USD".to_string(), "anything".to_string())
    );
    result_mocked.assert_success();

    // Transfer of tokens
    // let result = call!(
    //     alice,
    //     request_proxy.transfer_with_reference(
    //         "0xffffffffffffffff".to_string(),
    //         bob.account_id().try_into().unwrap(),
    //         U128::from(12),
    //         bob.account_id().try_into().unwrap(),
    //         U128::from(1)
    //     ),
    //     deposit = transfer_amount
    // );
    // result.assert_success();

    // println!(
    //     "test_transfer_with_reference > TeraGas burnt: {}",
    //     result.gas_burnt() as f64 / 1e12
    // );

    // // Check Alice balance
    // assert_eq_with_gas(
    //     to_yocto("50"), // 100 - 50
    //     alice.account().unwrap().amount,
    // );

    // Check Bob balance
    // assert_eq!(
    //     to_yocto("60"), // 10 + 50
    //     bob.account().unwrap().amount
    // );
}

/*
#[test]
fn test_transfer_with_receiver_does_not_exist() {
    let initial_alice_balance = to_yocto("100");
    let transfer_amount = to_yocto("50");

    let (_root, request_proxy, alice) = init_request_proxy(initial_alice_balance);

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy
            .transfer_with_reference("bob".try_into().unwrap(), "0xffffffffffffffff".to_string()),
        deposit = transfer_amount
    );
    assert!(result.is_ok());

    println!(
        "test_transfer_with_receiver_does_not_exist > TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    assert_one_promise_error(result.clone(), "account \"bob\" doesn't exist");

    assert_eq!(result.logs().len(), 1);
    assert!(result.logs()[0]
        .contains("Returning attached deposit of 50000000000000000000000000 to alice"));

    // Check Alice balance
    assert_eq_with_gas(
        to_yocto("100"), // Tokens returned
        alice.account().unwrap().amount,
    );
}

#[test]
fn test_transfer_with_not_enough_attached_gas() {
    let initial_alice_balance = to_yocto("100");
    let initial_bob_balance = to_yocto("10");
    let transfer_amount = to_yocto("50");
    let attached_gas: Gas = 30_000_000_000_000; // 30 TeraGas

    let (root, request_proxy, alice) = init_request_proxy(initial_alice_balance);

    let bob = root.create_user("bob".parse().unwrap(), initial_bob_balance);

    // Token transfer failed
    let result = alice.call(
        request_proxy.account_id(),
        "transfer_with_reference",
        &json!({
            "to": bob.account_id(),
            "payment_reference": "0xffffffffffffffff".to_string()
        })
        .to_string()
        .into_bytes(),
        attached_gas,
        transfer_amount,
    );
    // No successful outcome is expected
    assert!(!result.is_ok());

    println!(
        "test_transfer_with_not_enough_attached_gas > TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    assert_one_promise_error(result, "Not enough attach Gas to call this method");

    // Check Alice balance
    assert_eq_with_gas(to_yocto("100"), alice.account().unwrap().amount);
}
*/

fn alice_account() -> AccountId {
    "alice.near".to_string()
}

#[test]
fn test_transfer_with_invalid_reference_length() {
    let initial_bob_balance = to_yocto("100");
    let transfer_amount = to_yocto("500");

    let (root, alice, request_proxy, _fpo) = init();
    let bob = root.create_user("bob".parse().unwrap(), initial_bob_balance);
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = alice_account().try_into().unwrap();

    println!(
        "payment_address: {}, fee_address: {}",
        payment_address, fee_address
    );

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x11223344556677".to_string(),
            payment_address,
            U128::from(12),
            fee_address,
            U128::from(1)
        ),
        deposit = transfer_amount
    );
    // No successful outcome is expected
    assert!(!result.is_ok());

    println!(
        "test_transfer_with_invalid_parameter_length > TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    assert_one_promise_error(result, "Incorrect payment reference length");

    // Check Alice balance
    assert_eq_with_gas(to_yocto("1000"), alice.account().unwrap().amount);
}

/*
#[test]
fn test_transfer_with_invalid_reference_value() {
    let initial_alice_balance = to_yocto("100");
    let transfer_amount = to_yocto("50");

    let (_root, request_proxy, alice) = init_request_proxy(initial_alice_balance);

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference("bob".try_into().unwrap(), "0x123".to_string()),
        deposit = transfer_amount
    );
    // No successful outcome is expected
    assert!(!result.is_ok());

    println!(
        "test_transfer_with_invalid_reference_value > TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    assert_one_promise_error(result, "Payment reference value error");

    // Check Alice balance
    assert_eq_with_gas(to_yocto("100"), alice.account().unwrap().amount);
}
*/

fn bob_account() -> AccountId {
    "bob.near".to_string()
}

#[test]
fn test_mock_fpo_contract() {
    let (_root, alice, _request_proxy, fpo) = init();

    let price_entry = call!(
        alice,
        fpo.get_entry("NEAR/USD".to_string(), "useless".to_string())
    );
    price_entry.assert_success();

    debug_assert_eq!(
        &price_entry.unwrap_json_value()["price"].to_owned(),
        &"123000000".to_string()
    );
}

#[test]
fn test_transfer_low_deposit() {
    // TODO
}

#[test]
fn test_transfer_usd_near() {
    let initial_bob_balance = to_yocto("5000");
    let transfer_amount = to_yocto("500");

    let (root, alice, request_proxy, _fpo) = init();
    let bob = root.create_user("bob".parse().unwrap(), initial_bob_balance);
    let payment_address = bob.account_id().try_into().unwrap();

    let fee_address = alice_account().try_into().unwrap();

    println!(
        "payment_address: {}, fee_address: {}",
        payment_address, fee_address
    );

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x1122334455667788".to_string(),
            payment_address,
            U128::from(12),
            fee_address,
            U128::from(1)
        ),
        deposit = transfer_amount
    );
    result.assert_success();

    println!(
        "test_transfer_usd_near ==> TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    // Check Alice balance
    // assert_eq_with_gas(to_yocto("1000"), alice.account().unwrap().amount);
    // Check Bob balance
    // assert_eq!(bob.account().unwrap().amount, 12);
}
