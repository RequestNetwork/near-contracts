#![no_std]
#![feature(alloc)]
#![feature(global_allocator)]

extern crate alloc;

use near_sdk::{AccountId, Balance, ByteArray, NearBindgen, Promise};
use near_sdk::collections::Map;

#[global_allocator]
static ALLOC: near_sdk::wee_alloc::WeeAlloc<'_> = near_sdk::wee_alloc::WeeAlloc::INIT;

#[derive(Default, NearBindgen)]
#[near_bindgen(init = "init")]
pub struct ERC20ConversionProxy {
    // Mapping to store the oracle account
    oracle_accounts: Map<[u8; 32], AccountId>,
    // Mapping to store the provider account
    provider_accounts: Map<[u8; 32], AccountId>,
    // Owner of the contract
    owner: AccountId,
    // Token contract address
    token_address: AccountId,
    // Oracle contract address
    oracle_address: AccountId,
    // Provider contract address
    provider_address: AccountId,
}

#[derive(NearBindgen)]
#[near_bindgen(init = "init")]
struct PaymentRequest {
    amount: Balance,
    currency: [u8; 32],
    to: AccountId,
    fee_amount: Balance,
    fee_address: AccountId,
    max_rate_timespan: u64,
    payment_reference: [u8; 32],
}

#[near_bindgen]
impl ERC20ConversionProxy {
    pub fn new(
        owner: AccountId,
        token_address: AccountId,
        oracle_address: AccountId,
        provider_address: AccountId,
    ) -> Self {
        Self {
            oracle_accounts: Map::new(b"oa".to_vec()),
            provider_accounts: Map::new(b"pa".to_vec()),
            owner,
            token_address,
            oracle_address,
            provider_address,
        }
    }

    pub fn transfer_with_reference(
        &mut self,
        currency: [u8; 32],
        amount: Balance,
        to: AccountId,
        fee_amount: Balance,
        fee_address: AccountId,
        max_rate_timespan: u64,
        payment_reference: [u8; 32],
    ) -> Promise {
        // Check if the oracle's conversion rate is older than the max_rate_timespan value
        if max_rate_timespan != 0 && self.oracle_address.get_timestamp() > max_rate_timespan {
            // Throw an error if the conversion rate is too old
            Promise::revert()
        }

        // Get the current conversion rate from the oracle
        let conversion_rate = self.oracle_address.get_conversion_rate(currency);

        // Calculate the number of tokens to be transferred
        let token_amount = amount * conversion_rate;

        // Calculate the number of tokens for the fee
    let fee_token_amount = fee_amount * conversion_rate;

    // Transfer the tokens to the request issuer
    self.token_address.transfer(to, token_amount)?;

    // Transfer the tokens for the fee
    self.token_address.transfer(fee_address, fee_token_amount)?;

    // Emit the event
    self.transfer_with_reference(
        token_amount,
        fee_token_amount,
        PaymentRequest {
            amount,
            currency,
            to,
            fee_amount,
            fee_address,
            max_rate_timespan,
            payment_reference,
        },
    );

    // Return a successful promise}
    Promise::ok(())  }