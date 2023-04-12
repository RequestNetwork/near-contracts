use crate::utils::*;
use fungible_conversion_proxy::FungibleConversionProxyContract;
use mocks::fpo_oracle_mock::FPOContractContract;
use mocks::fungible_token_mock::FungibleTokenContractContract;
use near_sdk::json_types::{U128};
use near_sdk_sim::init_simulator;
use near_sdk_sim::runtime::GenesisConfig;
use near_sdk_sim::ContractAccount;
use near_sdk_sim::UserAccount;
use near_sdk_sim::{call, deploy, lazy_static_include, to_yocto};
use std::convert::TryInto;
use std::str;

near_sdk::setup_alloc!();

const PROXY_ID: &str = "fungible_conversion_proxy";
lazy_static_include::lazy_static_include_bytes! {
   PROXY_BYTES => "out/fungible_conversion_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
   MOCKED_BYTES => "out/mocks.wasm"
}

const DEFAULT_BALANCE: &str = "400000";

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

    let proxy = deploy!(
        contract: FungibleConversionProxyContract,
        contract_id: PROXY_ID,
        bytes: &PROXY_BYTES,
        signer_account: root,
        deposit: to_yocto("5"),
        init_method: new("mockedfpo".into(), "any".into())
    );

    let get_oracle_result = call!(root, proxy.get_oracle_account());
    get_oracle_result.assert_success();

    debug_assert_eq!(
        &get_oracle_result.unwrap_json_value().to_owned(),
        &"mockedfpo".to_string()
    );

    (
        account,
        empty_account_1,
        empty_account_2,
        proxy,
        ft_contract,
    )
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
        ft_contract.register_account(PROXY_ID.into())
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
        ft_contract.set_balance(PROXY_ID.into(), 0.into()) // 0 USDC.e
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
        ft_contract.set_balance(PROXY_ID.into(), send_amt)
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
fn test_transfer() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let get_args = call!(
        alice,
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
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
fn test_transfer_not_enough() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (_, _, _) = fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 1000 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    // The request is for 1000 USD, but alice only sends in 500 USDC.e
    let get_args = call!(
        alice,
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    assert_one_promise_error(result, "Deposit too small")
}

#[test]
fn test_transfer_receiver_send_failed() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

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
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
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
fn test_transfer_fee_receiver_send_failed() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

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
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
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
fn test_transfer_zero_usd() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(0); // 0 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 0 USD worth of USDC.e from alice to bob, with a 0 USD fee to builder
    let get_args = call!(
        alice,
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
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

    assert!(alice_balance_after == alice_balance_before);
    assert!(bob_balance_after == bob_balance_before);
    assert!(builder_balance_after == builder_balance_before);
}

#[test]
fn test_outdated_rate() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    // Constuct the `msg` argument using our contract
    // Transferring 100 USD worth of USDC.e from alice to bob, with a 2 USD fee to builder
    let get_args = call!(
        alice,
        proxy.get_transfer_with_reference_args(
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
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), msg)
    );
    assert_one_promise_error(result, "Conversion rate too old");
}
