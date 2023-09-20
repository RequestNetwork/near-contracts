use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{bs58, env, near_bindgen, Timestamp};

/**
 * Mocking the Switchboard feed parser contract for tests
 */

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct SwitchboardDecimal {
    pub mantissa: i128,
    pub scale: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct PriceEntry {
    pub result: SwitchboardDecimal,
    pub num_success: u32,
    pub num_error: u32,
    pub round_open_timestamp: Timestamp,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct SwitchboardIx {
    pub address: Vec<u8>,
    pub payer: Vec<u8>,
}

// For mocks: state of Switchboard feed parser
#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct SwitchboardFeedParser {}

#[near_bindgen]
impl SwitchboardFeedParser {
    #[allow(unused_variables)]
    pub fn aggregator_read(&self, ix: SwitchboardIx) -> Option<PriceEntry> {
        match &*bs58::encode(ix.address).into_string() {
            "testNEARtoUSD" => Some(PriceEntry {
                result: SwitchboardDecimal {
                    mantissa: i128::from(1234000),
                    scale: u8::from(6).into(),
                },
                num_success: 1,
                num_error: 0,
                round_open_timestamp: env::block_timestamp() - 10,
            }),
            _ => {
                panic!("InvalidAggregator")
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use near_sdk::{testing_env, AccountId, Balance, Gas, MockedBlockchain, VMContext};
    use near_sdk_sim::to_yocto;

    fn get_context(
        predecessor_account_id: AccountId,
        attached_deposit: Balance,
        prepaid_gas: Gas,
        is_view: bool,
    ) -> VMContext {
        VMContext {
            current_account_id: predecessor_account_id.clone(),
            signer_account_id: predecessor_account_id.clone(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 1,
            block_timestamp: 10,
            epoch_height: 1,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 10u64.pow(6),
            attached_deposit,
            prepaid_gas,
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
        }
    }
    #[test]
    fn aggregator_read() {
        let disp_vec = bs58::decode("E81iAUr7RPDUksAFtZxn7curbUVRy1Gps6sr6JnQALHx")
            .into_vec()
            .expect("!!")
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>()
            .join(",");
        println!("RESULT: {}", disp_vec);
        let context = get_context("alice.near".to_string(), to_yocto("1"), 10u64.pow(14), true);
        testing_env!(context);
        let contract = SwitchboardFeedParser::default();
        if let Some(result) = contract.aggregator_read(SwitchboardIx {
            address: bs58::decode("testNEARtoUSD").into_vec().expect("WRONG VEC"),
            payer: bs58::decode("anynearpayer").into_vec().expect("WRONG VEC"),
        }) {
            assert_eq!(result.result.mantissa, i128::from(1234000));
            assert_eq!(result.result.scale, 6);
        } else {
            panic!("NEAR/USD mock returned None")
        }
    }
    #[test]
    #[should_panic(expected = r#"InvalidAggregator"#)]
    fn missing_aggregator_read() {
        let disp_vec = bs58::decode("E81iAUr7RPDUksAFtZxn7curbUVRy1Gps6sr6JnQALHx")
            .into_vec()
            .expect("!!")
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>()
            .join(",");
        println!("RESULT: {}", disp_vec);
        let context = get_context("alice.near".to_string(), to_yocto("1"), 10u64.pow(14), true);
        testing_env!(context);
        let contract = SwitchboardFeedParser::default();
        contract.aggregator_read(SwitchboardIx {
            address: bs58::decode("wrongAggregator")
                .into_vec()
                .expect("WRONG VEC"),
            payer: bs58::decode("anynearpayer").into_vec().expect("WRONG VEC"),
        });
    }
}
