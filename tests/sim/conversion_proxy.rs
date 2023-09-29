use crate::utils::*;
use conversion_proxy::ConversionProxyContract;
use mocks::switchboard_feed_parser_mock::{valid_feed_key, SwitchboardFeedParserContract};
use near_sdk::json_types::{U128, U64};
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
   pub PROXY_BYTES => "target/wasm32-unknown-unknown/release/conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   pub MOCKED_BYTES => "target/wasm32-unknown-unknown/debug/mocks.wasm"
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
    let mut genesis = GenesisConfig::default();
    genesis.gas_price = 0;
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
        init_method: new("mockedswitchboard".into(), &valid_feed_key())
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

    let alice_balance = alice.account().unwrap().amount;
    assert!(alice_balance < initial_alice_balance);
    let spent_amount = initial_alice_balance - alice_balance;
    // 12'001.00 USD worth of NEAR / 1.234
    let expected_spent = to_yocto("12001") * 1000 / 1234;
    assert!(
        yocto_almost_eq(spent_amount, expected_spent),
        "\nSpent:    {spent_amount} \nExpected: {expected_spent} : Alice should have spent 12'000 + 1 USD worth of NEAR.",
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
    result.assert_one_promise_error("Incorrect payment reference length");

    // Check Alice balance
    assert_eq!(
        to_yocto(DEFAULT_BALANCE),
        alice.account().unwrap().amount,
        "Alice should not spend NEAR on invalid payment.",
    );
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
    result.assert_one_promise_error("Only payments denominated in USD are implemented for now");
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
    result.assert_success_one_log("Deposit too small for payment");

    assert_eq!(
        alice.account().unwrap().amount,
        initial_alice_balance,
        "Alice should not spend NEAR on a failed payment.",
    );

    assert_eq!(
        proxy.account().unwrap().amount,
        initial_contract_balance,
        "Contract's balance should be unchanged"
    );
    assert_eq!(
        builder.account().unwrap().amount,
        initial_bob_balance,
        "Builder's balance should be unchanged"
    );
}

#[test]
fn test_transfer_high_amounts() {
    let (alice, bob, builder, proxy, root) = init();
    let transfer_amount = to_yocto("20000000");
    // This high amount require a balance greater than the default one
    root.transfer(alice.account_id(), transfer_amount);
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let initial_builder_balance = builder.account().unwrap().amount;
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            // 1'200'00.00 USD (main)
            U128::from(120000000),
            USD.into(),
            fee_address,
            // 1.00 USD (fee)
            U128::from(100),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    result.assert_success();
    println!("{:?}", result);

    let alice_balance = alice.account().unwrap().amount;
    assert!(alice_balance < initial_alice_balance);
    let spent_amount = initial_alice_balance - alice_balance;
    // 1'200'001.00 USD worth of NEAR / 1.234
    let expected_spent = to_yocto("1200001") * 1000 / 1234;
    assert!(
        yocto_almost_eq(spent_amount, expected_spent),
        "\nSpent:    {spent_amount} \nExpected: {expected_spent} : Alice should have spent 1'200'000 + 1 USD worth of NEAR.",
    );

    assert!(bob.account().unwrap().amount > initial_bob_balance);
    let received_amount = bob.account().unwrap().amount - initial_bob_balance;
    assert_eq!(
        received_amount,
        // 1'200'000 USD / rate mocked
        to_yocto("1200000") * 1000 / 1234,
        "Bob should receive exactly 1'200'000 USD worth of NEAR."
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
fn test_transfer_with_wrong_feed_address() {
    let (alice, bob, builder, proxy, root) = init();

    let result = call!(alice, proxy.get_encoded_feed_address());
    result.assert_success();

    let result = call!(
        root,
        proxy.set_feed_address(&"7igqhpGQ8xPpyjQ4gMHhXRvtZcrKSGJkdKDJYBiPQgcb".to_string())
    );
    result.assert_success();

    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let initial_contract_balance = proxy.account().unwrap().amount;
    let transfer_amount = to_yocto("100000");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

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
    result.assert_success_one_log("ERR_FAILED_ORACLE_FETCH");

    assert_eq!(
        alice.account().unwrap().amount,
        initial_alice_balance,
        "Alice should not spend NEAR on a wrong feed address payment.",
    );

    assert_eq!(
        proxy.account().unwrap().amount,
        initial_contract_balance,
        "Contract's balance should be unchanged"
    );
    assert_eq!(
        builder.account().unwrap().amount,
        initial_bob_balance,
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
    assert_eq!(
        initial_alice_balance, alice_balance,
        "Alice should not spend NEAR on a 0 USD payment.",
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
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_proxy_balance = proxy.account().unwrap().amount;
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        proxy.transfer_with_reference(
            PAYMENT_REF.into(),
            payment_address,
            U128::from(120000),
            USD.into(),
            fee_address,
            U128::from(0),
            // The mocked rate is 10 nanoseconds old
            U64::from(1)
        ),
        deposit = transfer_amount
    );
    result.assert_success_one_log("Conversion rate too old");

    assert_eq!(
        initial_proxy_balance,
        proxy.account().unwrap().amount,
        "Contract's balance should be unchanged"
    );
    assert_eq!(
        initial_alice_balance,
        alice.account().unwrap().amount,
        "Alice should not spend NEAR on an outdated rate payment.",
    );
}
