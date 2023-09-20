use crate::utils::*;
use conversion_proxy::ConversionProxyContract;
use mocks::switchboard_feed_parser_mock::SwitchboardFeedParserContract;
use near_sdk::json_types::{U128, U64};
use near_sdk::Balance;
use near_sdk_sim::init_simulator;
use near_sdk_sim::runtime::GenesisConfig;
use near_sdk_sim::ContractAccount;
use near_sdk_sim::UserAccount;
use near_sdk_sim::{call, deploy, lazy_static_include, to_yocto};
use std::convert::TryInto;
use std::str;

near_sdk::setup_alloc!();

const PROXY_ID: &str = "conversion_proxy";
lazy_static_include::lazy_static_include_bytes! {
   PROXY_BYTES => "target/wasm32-unknown-unknown/release/conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   MOCKED_BYTES => "target/wasm32-unknown-unknown/debug/mocks.wasm"
}

const DEFAULT_BALANCE: &str = "400000";
const USD: &str = "USD";
const PAYMENT_REF: &str = "0x1122334455667788";

// Initialize test environment with 3 accounts (alice, bob, builder), a conversion mock, and its owner account.
fn init() -> (
    UserAccount,
    UserAccount,
    UserAccount,
    ContractAccount<ConversionProxyContract>,
    UserAccount,
) {
    let genesis = GenesisConfig::default();
    let root = init_simulator(Some(genesis));

    deploy!(
        contract: SwitchboardFeedParserContract,
        contract_id: "mockedswitchboard".to_string(),
        bytes: &MOCKED_BYTES,
        signer_account: root,
        deposit: to_yocto("7")
    );

    let account = root.create_user("alice".to_string(), to_yocto(DEFAULT_BALANCE));

    let zero_balance: u128 = 1820000000000000000000;
    let empty_account_1 = root.create_user("bob".parse().unwrap(), zero_balance);
    let empty_account_2 = root.create_user("builder".parse().unwrap(), zero_balance);

    let proxy = deploy!(
        contract: ConversionProxyContract,
        contract_id: PROXY_ID,
        bytes: &PROXY_BYTES,
        signer_account: root,
        deposit: to_yocto("5"),
        init_method: new("mockedswitchboard".into(), bs58::decode("testNEARtoUSD").into_vec().expect("WRONG VEC"))
    );

    let set_feed_payer_result = call!(root, proxy.set_feed_payer());
    set_feed_payer_result.assert_success();
    let get_parser_result = call!(root, proxy.get_feed_parser());
    get_parser_result.assert_success();

    debug_assert_eq!(
        &get_parser_result.unwrap_json_value().to_owned(),
        &"mockedswitchboard".to_string()
    );

    (account, empty_account_1, empty_account_2, proxy, root)
}

#[test]
fn test_transfer() {
    let (alice, bob, builder, proxy, _) = init();
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let initial_builder_balance = builder.account().unwrap().amount;
    let transfer_amount = to_yocto("200000");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            // 12000.00 USD (main)
            U128::from(1200000),
            USD.into(),
            fee_address,
            // 1.00 USD (fee)
            U128::from(100),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    result.assert_success();

    println!(
        "test_transfer_usd_near ==> TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    let alice_balance = alice.account().unwrap().amount;
    assert!(alice_balance < initial_alice_balance);
    let spent_amount = initial_alice_balance - alice_balance;
    // 12'001.00 USD worth of NEAR / 1.234
    let expected_spent = to_yocto("12001") * 1000 / 1234;
    assert!(
        spent_amount - expected_spent < to_yocto("0.005"),
        "Alice should spend 12'000 + 1 USD worth of NEAR (+ gas). Spent {spent_amount}, expected: {expected_spent}.",
    );

    assert!(bob.account().unwrap().amount > initial_bob_balance);
    let received_amount = bob.account().unwrap().amount - initial_bob_balance;
    assert_eq!(
        received_amount,
        // 12'000 USD / rate mocked
        to_yocto("12000") * 1000 / 1234,
        "Bob should receive exactly 12'000 USD worth of NEAR."
    );

    assert!(builder.account().unwrap().amount > initial_builder_balance);
    let received_amount = builder.account().unwrap().amount - initial_builder_balance;
    assert_eq!(
        received_amount,
        // 1 USD / rate mocked
        to_yocto("1") * 1000 / 1234,
        "Builder should receive exactly 1 USD worth of NEAR"
    );
}

#[test]
fn test_transfer_with_invalid_reference_length() {
    let transfer_amount = to_yocto("500");

    let (alice, bob, builder, proxy, _) = init();
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    // Token transfer failed
    let result = call!(
        alice,
        proxy.transfer_with_reference(
            "0x11223344556677".to_string(),
            payment_address,
            U128::from(12),
            USD.into(),
            fee_address,
            U128::from(1),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    assert_one_promise_error(result.clone(), "Incorrect payment reference length");

    println!(
        "test_transfer_with_invalid_parameter_length > TeraGas burnt: {}",
        result.gas_burnt() as f64 / 1e12
    );

    // Check Alice balance
    assert_eq_with_gas(to_yocto(DEFAULT_BALANCE), alice.account().unwrap().amount);
}

#[test]
fn test_transfer_with_wrong_currency() {
    let (alice, bob, builder, proxy, _) = init();
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    // Token transfer failed
    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            U128::from(1200),
            String::from("WRONG"),
            fee_address,
            U128::from(100),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    assert_one_promise_error(
        result,
        "Only payments denominated in USD are implemented for now",
    );
}

#[test]
fn test_transfer_with_low_deposit() {
    let (alice, bob, builder, proxy, _) = init();
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let initial_contract_balance = proxy.account().unwrap().amount;
    let transfer_amount = to_yocto("1000");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    assert!(
        transfer_amount / to_yocto("1") > 0,
        "Sanity check: this test is only relevant with high transfer amounts"
    );
    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            U128::from(2000000),
            USD.into(),
            fee_address,
            U128::from(0),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    result.assert_success();
    assert_eq!(result.logs().len(), 1, "Wrong number of logs");
    assert!(result.logs()[0].contains("Deposit too small for payment"));

    // Alice's balance is slightly impacted by gas, hence we divide by 1 NEAR
    assert!(
        alice.account().unwrap().amount / to_yocto("1") == initial_alice_balance / to_yocto("1"),
        "Alice should not spend NEAR on a failed payment",
    );

    // The contract's balance is slightly impacted by execution, hence we divide by 1 NEAR
    assert!(
        proxy.account().unwrap().amount / to_yocto("1") == initial_contract_balance / to_yocto("1"),
        "Contract's balance should be unchanged"
    );
    assert!(
        builder.account().unwrap().amount == initial_bob_balance,
        "Builder's balance should be unchanged"
    );
}

#[test]
fn test_transfer_zero_usd() {
    let (alice, bob, builder, proxy, _) = init();
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            U128::from(0),
            USD.into(),
            fee_address,
            U128::from(0),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    result.assert_success();

    let alice_balance = alice.account().unwrap().amount;
    assert!(alice_balance < initial_alice_balance);
    let spent_amount = initial_alice_balance - alice_balance;
    assert!(
        spent_amount < to_yocto("0.005"),
        "Alice should not spend NEAR on a 0 USD payment",
    );

    assert!(
        bob.account().unwrap().amount == initial_bob_balance,
        "Bob's balance should be unchanged"
    );
    assert!(
        builder.account().unwrap().amount == initial_bob_balance,
        "Builder's balance should be unchanged"
    );
}

#[test]
fn test_outdated_rate() {
    let (alice, bob, builder, proxy, _) = init();
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            U128::from(0),
            USD.into(),
            fee_address,
            U128::from(0),
            // The mocked rate is 10 nanoseconds old
            U64::from(1)
        ),
        deposit = transfer_amount
    );
    assert_one_promise_error(result, "Conversion rate too old");
}
