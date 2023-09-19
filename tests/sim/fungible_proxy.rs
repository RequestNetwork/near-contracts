use crate::utils::*;
use fungible_proxy::FungibleProxyContract;
use fungible_proxy::PaymentArgs;
use mocks::fungible_token_mock::FungibleTokenContractContract;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk_sim::init_simulator;
use near_sdk_sim::runtime::GenesisConfig;
use near_sdk_sim::ContractAccount;
use near_sdk_sim::UserAccount;
use near_sdk_sim::{call, deploy, lazy_static_include, to_yocto};
use std::convert::TryInto;
use std::ops::Sub;
use std::str;

near_sdk::setup_alloc!();

const PROXY_ID: &str = "fungible_proxy";
lazy_static_include::lazy_static_include_bytes! {
   PROXY_BYTES => "target/wasm32-unknown-unknown/release/fungible_proxy.wasm"
}
lazy_static_include::lazy_static_include_bytes! {
    MOCKED_BYTES => "target/wasm32-unknown-unknown/debug/mocks.wasm"
}

const DEFAULT_BALANCE: &str = "400000";

// Initialize test environment with 3 accounts (alice, bob, builder), a fungible conversion mock, and a fungible token mock.
fn init_fungible() -> (
    UserAccount,
    UserAccount,
    UserAccount,
    ContractAccount<FungibleProxyContract>,
    ContractAccount<FungibleTokenContractContract>,
) {
    let genesis = GenesisConfig::default();
    let root = init_simulator(Some(genesis));

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
        contract: FungibleProxyContract,
        contract_id: PROXY_ID,
        bytes: &PROXY_BYTES,
        signer_account: root,
        deposit: to_yocto("5")
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
    call!(builder, ft_contract.register_account(PROXY_ID.into()));

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
    call!(alice, ft_contract.set_balance(PROXY_ID.into(), send_amt));
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

    let args = PaymentArgs {
        fee_address: builder.account_id().try_into().unwrap(),
        fee_amount: 2000000.into(), // 2 USDC.e
        payment_reference: "abc7c8bb1234fd11".into(),
        to: bob.account_id().try_into().unwrap(),
    };

    let result = call!(
        ft_contract.user_account,
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    assert_eq!(result.logs().len(), 1, "Wrong number of logs");
    let expected_log = json!({
        "amount": "498000000", // 500 USDC.e - 2 USDC.e fee
        "token_address": "mockedft",
        "fee_address": "builder",
        "fee_amount": "2000000",
        "payment_reference": "abc7c8bb1234fd11",
        "to": "bob",
    })
    .to_string();
    assert_eq!(result.logs()[0], expected_log);

    // The mocked fungible token does not handle change
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();
    assert!(change == 0);

    assert_spent(alice, alice_balance_before, send_amt.into(), &ft_contract);
    assert_received(bob, bob_balance_before, 498000000, &ft_contract);
    assert_received(builder, builder_balance_before, 2000000, &ft_contract);
}

#[test]
fn transfer_less_than_fee_amount() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(500000000); // 500 USDC.e
    let (_, _, _) = fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    let args = PaymentArgs {
        fee_address: "builder".try_into().unwrap(),
        fee_amount: 500100000.into(), // 500.10 USDC.e
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
    };

    let result = call!(
        ft_contract.user_account,
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    assert_one_promise_error(result, "amount smaller than fee_amount")
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

    let args = PaymentArgs {
        fee_address: builder.account_id().try_into().unwrap(),
        fee_amount: 2000000.into(), // 2 USDC.e
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
    };

    let result = call!(
        ft_contract.user_account,
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    assert_eq!(result.logs().len(), 1, "Wrong number of logs");
    assert_eq!(result.logs()[0], "Transfer failed to bob or builder. Returning attached amount of 500000000 of token mockedft to alice");

    // The mocked fungible token does not handle change
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();

    let result = call!(alice, ft_contract.ft_balance_of(alice.account_id()));
    let alice_balance_after = result.unwrap_json::<U128>().0 + change;

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

    let args = PaymentArgs {
        fee_address: "builder".try_into().unwrap(),
        fee_amount: 200.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
    };

    let result = call!(
        ft_contract.user_account,
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    assert_eq!(result.logs().len(), 1, "Wrong number of logs");
    assert_eq!(result.logs()[0], "Transfer failed to bob or builder. Returning attached amount of 500000000 of token mockedft to alice");

    // The mocked fungible token does not handle change
    let change = result.unwrap_json::<String>().parse::<u128>().unwrap();
    assert_eq!(change, 500000000);

    assert_unchanged_balance(
        alice,
        alice_balance_before.sub(change),
        &ft_contract,
        "Alice",
    );
}

#[test]
fn test_transfer_zero_usd() {
    let (alice, bob, builder, proxy, ft_contract) = init_fungible();

    let send_amt = U128::from(0); // 0 USDC.e
    let (alice_balance_before, bob_balance_before, builder_balance_before) =
        fungible_transfer_setup(&alice, &bob, &builder, &ft_contract, send_amt);

    let args = PaymentArgs {
        fee_address: "builder".try_into().unwrap(),
        fee_amount: 0.into(),
        payment_reference: "abc7c8bb1234fd12".into(),
        to: bob.account_id().try_into().unwrap(),
    };
    let result = call!(
        ft_contract.user_account,
        proxy.ft_on_transfer(alice.account_id(), send_amt.0.to_string(), args.into())
    );
    result.assert_success();
    assert_eq!(result.logs().len(), 1, "Wrong number of logs");
    let expected_log = json!({
        "amount": "0",
        "token_address": "mockedft",
        "fee_address": "builder",
        "fee_amount": "0",
        "payment_reference": "abc7c8bb1234fd12",
        "to": "bob",
    })
    .to_string();
    assert_eq!(result.logs()[0], expected_log);

    assert_unchanged_balance(alice, alice_balance_before, &ft_contract, "Alice");
    assert_unchanged_balance(bob, bob_balance_before, &ft_contract, "Bob");
    assert_unchanged_balance(builder, builder_balance_before, &ft_contract, "Builder");
}
