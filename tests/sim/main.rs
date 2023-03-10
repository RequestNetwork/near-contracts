use crate::utils::*;
use conversion_proxy::ConversionProxyContract;
use fungible_conversion_proxy::{FungibleConversionProxyContract, PaymentArgs, UnfixedPaymentArgs};
use mocks::fpo_oracle_mock::FPOContractContract;
use mocks::fungible_token_mock::FungibleTokenContractContract;
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

const PROXY_ID: &str = "request_proxy";
const FUNGIBLE_PROXY_ID: &str = "fungible_request_proxy";
lazy_static_include::lazy_static_include_bytes! {
   REQUEST_PROXY_BYTES => "out/conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   FUNGIBLE_REQUEST_PROXY_BYTES => "out/fungible_conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   MOCKED_BYTES => "out/mocks.wasm"
}

mod utils;

const DEFAULT_BALANCE: &str = "400000";

// Initialize test environment with 3 accounts (alice, bob, builder) and a conversion mock.
fn init() -> (
    UserAccount,
    UserAccount,
    UserAccount,
    ContractAccount<ConversionProxyContract>,
) {
    let genesis = GenesisConfig::default();
    let root = init_simulator(Some(genesis));

    deploy!(
        contract: FPOContractContract,
        contract_id: "mockedfpo".to_string(),
        bytes: &MOCKED_BYTES,
        signer_account: root,
        deposit: to_yocto("7")
    );

    let account = root.create_user("alice".to_string(), to_yocto(DEFAULT_BALANCE));

    let zero_balance: u128 = 1820000000000000000000;
    let empty_account_1 = root.create_user("bob".parse().unwrap(), zero_balance);
    let empty_account_2 = root.create_user("builder".parse().unwrap(), zero_balance);

    let request_proxy = deploy!(
        contract: ConversionProxyContract,
        contract_id: PROXY_ID,
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

    (account, empty_account_1, empty_account_2, request_proxy)
}

// Initialize test environment with 3 accounts (alice, bob, builder), a fungible conversion mock, and a fungible token mock.
fn init_fungible() -> (
    UserAccount,
    UserAccount,
    UserAccount,
    ContractAccount<FungibleConversionProxyContract>,
    ContractAccount<FungibleTokenContractContract>,
) {
    let genesis = GenesisConfig::default();
    let root = init_simulator(Some(genesis));

    deploy!(
        contract: FPOContractContract,
        contract_id: "mockedfpo".to_string(),
        bytes: &MOCKED_BYTES,
        signer_account: root,
        deposit: to_yocto("7")
    );

    let ft_contract = deploy!(
        contract: FungibleTokenContractContract,
        contract_id: "mockedft".to_string(),
        bytes: &MOCKED_BYTES,
        signer_account: root,
        deposit: to_yocto("7")
    );

    let account = root.create_user("alice".to_string(), to_yocto(DEFAULT_BALANCE));
    let empty_account_1 = root.create_user("bob".parse().unwrap(), to_yocto(DEFAULT_BALANCE));
    let empty_account_2 = root.create_user("builder".parse().unwrap(), to_yocto(DEFAULT_BALANCE));

    let fungible_request_proxy = deploy!(
        contract: FungibleConversionProxyContract,
        contract_id: FUNGIBLE_PROXY_ID,
        bytes: &FUNGIBLE_REQUEST_PROXY_BYTES,
        signer_account: root,
        deposit: to_yocto("5"),
        init_method: new("mockedfpo".into(), "any".into())
    );

    let get_oracle_result = call!(root, fungible_request_proxy.get_oracle_account());
    get_oracle_result.assert_success();

    debug_assert_eq!(
        &get_oracle_result.unwrap_json_value().to_owned(),
        &"mockedfpo".to_string()
    );

    (
        account,
        empty_account_1,
        empty_account_2,
        fungible_request_proxy,
        ft_contract,
    )
}

#[test]
fn test_transfer_usd_near() {
    let (alice, bob, builder, request_proxy) = init();
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let initial_builder_balance = builder.account().unwrap().amount;
    let transfer_amount = to_yocto("200000");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();
    const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000;

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x1122334455667788".to_string(),
            payment_address,
            // 12000.00 USD (main)
            U128::from(1200000),
            String::from("USD"),
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
        "Alice should spend 12'000 + 1 USD worth of NEAR (+ gas)",
    );
    println!("diff: {}", (spent_amount - expected_spent) / ONE_NEAR);

    assert!(bob.account().unwrap().amount > initial_bob_balance);
    let received_amount = bob.account().unwrap().amount - initial_bob_balance;
    assert_eq!(
        received_amount,
        // 12'000 USD / rate mocked
        to_yocto("12000") * 1000 / 1234,
        "Bob should receive exactly 12000 USD worth of NEAR"
    );

    assert!(builder.account().unwrap().amount > initial_builder_balance);
    let received_amount = builder.account().unwrap().amount - initial_builder_balance;
    assert_eq!(
        received_amount,
        // 1 USD
        to_yocto("1") * 1000 / 1234,
        "Builder should receive exactly 1 USD worth of NEAR"
    );
}

#[test]
fn test_transfer_with_invalid_reference_length() {
    let transfer_amount = to_yocto("500");

    let (alice, bob, builder, request_proxy) = init();
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x11223344556677".to_string(),
            payment_address,
            U128::from(12),
            String::from("USD"),
            fee_address,
            U128::from(1),
            U64::from(0)
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
    assert_eq_with_gas(to_yocto(DEFAULT_BALANCE), alice.account().unwrap().amount);
}

#[test]
fn test_transfer_with_wrong_currency() {
    let (alice, bob, builder, request_proxy) = init();
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    // Token transfer failed
    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x1122334455667788".to_string(),
            payment_address,
            U128::from(1200),
            String::from("WRONG"),
            fee_address,
            U128::from(100),
            U64::from(0)
        ),
        deposit = transfer_amount
    );
    assert_one_promise_error(result, "ERR_INVALID_ORACLE_RESPONSE");
}

#[test]
fn test_transfer_zero_usd_near() {
    let (alice, bob, builder, request_proxy) = init();
    let initial_alice_balance = alice.account().unwrap().amount;
    let initial_bob_balance = bob.account().unwrap().amount;
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x1122334455667788".to_string(),
            payment_address,
            U128::from(0),
            String::from("USD"),
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
    let (alice, bob, builder, request_proxy) = init();
    let transfer_amount = to_yocto("100");
    let payment_address = bob.account_id().try_into().unwrap();
    let fee_address = builder.account_id().try_into().unwrap();

    let result = call!(
        alice,
        request_proxy.transfer_with_reference(
            "0x1122334455667788".to_string(),
            payment_address,
            U128::from(0),
            String::from("USD"),
            fee_address,
            U128::from(0),
            // The mocked rate is 10 nanoseconds old
            U64::from(1)
        ),
        deposit = transfer_amount
    );
    assert_one_promise_error(result, "Conversion rate too old");
}

// Helper function for setting up fungible token transfer tests
fn fungible_transfer_setup(
    alice: &UserAccount,
    bob: &UserAccount,
    builder: &UserAccount,
    ft_contract: &ContractAccount<FungibleTokenContractContract>,
    send_amt: U128,
) -> (u128, u128, u128) {
    // Register alice, bob, builder, and the contract with the fungible token
    call!(alice, ft_contract.register_account(alice.account_id()));
    call!(bob, ft_contract.register_account(bob.account_id()));
    call!(builder, ft_contract.register_account(builder.account_id()));
    call!(
        builder,
        ft_contract.register_account(FUNGIBLE_PROXY_ID.into())
    );

    // Set initial balances
    call!(
        alice,
        ft_contract.set_balance(alice.account_id(), 1000000000.into()) // 1000 USDC.e
    );
    call!(
        bob,
        ft_contract.set_balance(bob.account_id(), 1000000.into()) // 1 USDC.e
    );
    call!(
        builder,
        ft_contract.set_balance(builder.account_id(), 0.into()) // 0 USDC.e
    );
    call!(
        builder,
        ft_contract.set_balance(FUNGIBLE_PROXY_ID.into(), 0.into()) // 0 USDC.e
    );

    let alice_balance_before = call!(alice, ft_contract.ft_balance_of(alice.account_id()))
        .unwrap_json::<U128>()
        .0;
    let bob_balance_before = call!(bob, ft_contract.ft_balance_of(bob.account_id()))
        .unwrap_json::<U128>()
        .0;
    let builder_balance_before = call!(builder, ft_contract.ft_balance_of(builder.account_id()))
        .unwrap_json::<U128>()
        .0;

    // In real usage, the user calls `ft_transfer_call` on the token contract, which calls `ft_on_transfer` on our contract
    // The token contract will transfer the specificed tokens from the caller to our contract before calling our contract
    call!(
        alice,
        ft_contract.set_balance(FUNGIBLE_PROXY_ID.into(), send_amt)
    );
    call!(
        alice,
        ft_contract.set_balance(
            alice.account_id(),
            U128::from(alice_balance_before - send_amt.0)
        )
    );

    (
        alice_balance_before,
        bob_balance_before,
        builder_balance_before,
    )
}

#[test]
fn test_transfer_usd_fungible() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let args = UnfixedPaymentArgs {
        amount: 10000.into(),
        currency: "USD".into(),
        fee_address: "builder".to_string().try_into().unwrap(),
        fee_amount: 200.into(),
        max_rate_timespan: 0.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
    };

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    let alice_balance_after = call!(alice, ft_contract.ft_balance_of(alice.account_id()))
        .unwrap_json::<U128>()
        .0
        + change;
    let bob_balance_after = call!(bob, ft_contract.ft_balance_of(bob.account_id()))
        .unwrap_json::<U128>()
        .0;
    let builder_balance_after = call!(builder, ft_contract.ft_balance_of(builder.account_id()))
        .unwrap_json::<U128>()
        .0;

    // USDC.e has 6 decimals
    let total_usdce_amount = 102 * 1000000; // 102 USD
    let payment_usdce_amount = 100 * 1000000; // 100 USD
    let fee_usdce_amount = 2 * 1000000; // 2 USD

    // The price of USDC.e returned by the oracle is 999900 with 6 decimals, so 1 USDC.e = 999900/1000000 USD = 0.9999 USD
    // Here we need it the other way (USD in terms of USDC.e), so 1 USD = 1000000/999900 USDC.e = 1.00010001 USDC.e
    let rate_numerator = 1000000;
    let rate_denominator = 999900;

    assert!(alice_balance_after < alice_balance_before);
    let spent_amount = alice_balance_before - alice_balance_after;
    let expected_spent = total_usdce_amount * rate_numerator / rate_denominator;
    assert!(spent_amount == expected_spent);

    assert!(bob_balance_after > bob_balance_before);
    let received_amount = bob_balance_after - bob_balance_before;
    let expected_received = payment_usdce_amount * rate_numerator / rate_denominator;
    assert!(received_amount == expected_received);

    assert!(builder_balance_after > builder_balance_before);
    let received_amount = builder_balance_after - builder_balance_before;
    let expected_received = fee_usdce_amount * rate_numerator / rate_denominator;
    assert!(received_amount == expected_received);
}


#[test]
fn test_transfer_usd_fungible_ignore_fixed_rate() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let args = PaymentArgs {
        amount: 10000.into(),
        currency: "USD".into(),
        fee_address: "builder".to_string().try_into().unwrap(),
        fee_amount: 200.into(),
        max_rate_timespan: 0.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
        fixed_rate: Some(1000000.into()) // Should be ignored
    };

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    let alice_balance_after = call!(alice, ft_contract.ft_balance_of(alice.account_id()))
        .unwrap_json::<U128>()
        .0
        + change;
    let bob_balance_after = call!(bob, ft_contract.ft_balance_of(bob.account_id()))
        .unwrap_json::<U128>()
        .0;
    let builder_balance_after = call!(builder, ft_contract.ft_balance_of(builder.account_id()))
        .unwrap_json::<U128>()
        .0;

    // USDC.e has 6 decimals
    let total_usdce_amount = 102 * 1000000; // 102 USD
    let payment_usdce_amount = 100 * 1000000; // 100 USD
    let fee_usdce_amount = 2 * 1000000; // 2 USD

    // The price of USDC.e returned by the oracle is 999900 with 6 decimals, so 1 USDC.e = 999900/1000000 USD = 0.9999 USD
    // Here we need it the other way (USD in terms of USDC.e), so 1 USD = 1000000/999900 USDC.e = 1.00010001 USDC.e
    let rate_numerator = 1000000;
    let rate_denominator = 999900;

    assert!(alice_balance_after < alice_balance_before);
    let spent_amount = alice_balance_before - alice_balance_after;
    let expected_spent = total_usdce_amount * rate_numerator / rate_denominator;
    assert!(spent_amount == expected_spent);

    assert!(bob_balance_after > bob_balance_before);
    let received_amount = bob_balance_after - bob_balance_before;
    let expected_received = payment_usdce_amount * rate_numerator / rate_denominator;
    assert!(received_amount == expected_received);

    assert!(builder_balance_after > builder_balance_before);
    let received_amount = builder_balance_after - builder_balance_before;
    let expected_received = fee_usdce_amount * rate_numerator / rate_denominator;
    assert!(received_amount == expected_received);
}


#[test]
fn test_transfer_fungible_fix_rate() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Test with USDC.other token, unknown to the oracle
    let set_symbol_result = call!(builder, ft_contract.set_symbol("USDC.other".into()));
    set_symbol_result.assert_success();

    let args = PaymentArgs {
        amount: 10000.into(),
        currency: "USD".into(),
        fee_address: "builder".to_string().try_into().unwrap(),
        fee_amount: 0.into(),
        max_rate_timespan: 0.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
        fixed_rate: Some(1000000.into())
    };

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    
    // Need to mock the fact that fungible tokens would give the change back
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    // USDC.e has 6 decimals
    let total_usdcother_amount = 100 * 1000000; // 100 USD
    let payment_usdcother_amount = 100 * 1000000; // 100 USD

    let expected_spent_amount = total_usdcother_amount; // * rate_numerator / rate_denominator;
    let expected_received_amount = payment_usdcother_amount; // * rate_numerator / rate_denominator;
    assert_spent(alice, alice_balance_before, expected_spent_amount + change, &ft_contract); 
    assert_received(bob, bob_balance_before, expected_received_amount, &ft_contract);
    assert_received(builder, builder_balance_before, 0, &ft_contract);
}


#[test]
fn test_transfer_fungible_failing_oracle() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Test with USDC.other token, unknown to the oracle
    let set_symbol_result = call!(builder, ft_contract.set_symbol("FAIL".into()));
    set_symbol_result.assert_success();

    let args = PaymentArgs {
        amount: 10000.into(),
        currency: "USD".into(),
        fee_address: "builder".to_string().try_into().unwrap(),
        fee_amount: 0.into(),
        max_rate_timespan: 0.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
        fixed_rate: Some(1000000.into())
    };

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    
    assert_last_promise_error(result, "ERR_FAILED_ORACLE_FETCH");

    assert_unchanged_balance(alice, alice_balance_before, &ft_contract, "Alice");
    assert_unchanged_balance(bob, bob_balance_before, &ft_contract, "Bob");
    assert_unchanged_balance(builder, builder_balance_before, &ft_contract, "Builder");
}


#[test]
fn test_transfer_usd_fungible_not_enough() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (_, _, _) = fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 1000 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    // The request is for 1000 USD, but alice only sends in 500 USDC.e
    let get_args = call!(
        alice,
        fungible_request_proxy.get_transfer_with_reference_args(
            100000.into(), // 1000 USD
            "USD".into(),
            "builder".to_string().try_into().unwrap(),
            200.into(), // 2 USD
            0.into(),
            "abc7c8bb1234fd12".into(),
            bob.account_id().try_into().unwrap()
        )
    );
    get_args.assert_success();
    let msg = get_args.unwrap_json::<String>().replace("\\", "");

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    assert_one_promise_error(result, "Deposit too small")
}

#[test]
fn test_transfer_usd_fungible_receiver_send_failed() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, _, _) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Previous line registers all accounts, so we unregister bob here
    // Receiver is not registered with the token contract, so sending to it will fail
    call!(bob, ft_contract.unregister_account(bob.account_id()));

    // Constuct the `msg` argument using our contract
    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let get_args = call!(
        alice,
        fungible_request_proxy.get_transfer_with_reference_args(
            10000.into(), // 100 USD
            "USD".into(),
            "builder".to_string().try_into().unwrap(),
            200.into(), // 2 USD
            0.into(),
            "abc7c8bb1234fd12".into(),
            bob.account_id().try_into().unwrap()
        )
    );
    get_args.assert_success();
    let msg = get_args.unwrap_json::<String>().replace("\\", "");

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    result.assert_success();
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    let alice_balance_after = call!(alice, ft_contract.ft_balance_of(alice.account_id()))
        .unwrap_json::<U128>()
        .0
        + change;

    // Ensure no balance changes / all funds returned to sender
    assert!(alice_balance_after == alice_balance_before);
}

#[test]
fn test_transfer_usd_fungible_fee_receiver_send_failed() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, _, _) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Previous line registers all accounts, so we unregister builder here
    // Fee_receiver is not registered with the token contract, so sending to it will fail
    call!(
        builder,
        ft_contract.unregister_account(builder.account_id())
    );

    // Constuct the `msg` argument using our contract
    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let get_args = call!(
        alice,
        fungible_request_proxy.get_transfer_with_reference_args(
            10000.into(), // 100 USD
            "USD".into(),
            "builder".to_string().try_into().unwrap(),
            200.into(), // 2 USD
            0.into(),
            "abc7c8bb1234fd12".into(),
            bob.account_id().try_into().unwrap()
        )
    );
    get_args.assert_success();
    let msg = get_args.unwrap_json::<String>().replace("\\", "");

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    result.assert_success();
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    let alice_balance_after = call!(alice, ft_contract.ft_balance_of(alice.account_id()))
        .unwrap_json::<U128>()
        .0
        + change;

    // Ensure no balance changes / all funds returned to sender
    assert!(alice_balance_after == alice_balance_before);
}

#[test]
fn test_zero_usd_fungible() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(0); // 0 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 0 USD worth of USDC.e from alice to bob, with a 0 USD fee to builder
    let get_args = call!(
        alice,
        fungible_request_proxy.get_transfer_with_reference_args(
            0.into(), // 0 USD
            "USD".into(),
            "builder".to_string().try_into().unwrap(),
            0.into(), // 0 USD
            0.into(),
            "abc7c8bb1234fd12".into(),
            bob.account_id().try_into().unwrap()
        )
    );
    get_args.assert_success();
    let msg = get_args.unwrap_json::<String>().replace("\\", "");

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg),
        gas = 1_500_000_000_000_000
    );
    result.assert_success();

    assert_unchanged_balance(alice, alice_balance_before, &ft_contract, "Alice");
    assert_unchanged_balance(bob, bob_balance_before, &ft_contract, "Bob");
    assert_unchanged_balance(builder, builder_balance_before, &ft_contract, "Builder");
}

#[test]
fn test_outdated_rate_fungible() {
    let (alice, bob, builder, fungible_request_proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let get_args = call!(
        alice,
        fungible_request_proxy.get_transfer_with_reference_args(
            10000.into(), // 100 USD
            "USD".into(),
            "builder".to_string().try_into().unwrap(),
            200.into(), // 2 USD
            1.into(),   // 1 ns
            "abc7c8bb1234fd12".into(),
            bob.account_id().try_into().unwrap()
        )
    );
    get_args.assert_success();
    let msg = get_args.unwrap_json::<String>().replace("\\", "");

    let result = call!(
        ft_contract.user_account,
        fungible_request_proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    assert_one_promise_error(result, "Conversion rate too old");
}
