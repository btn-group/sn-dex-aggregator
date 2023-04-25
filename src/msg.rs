use crate::state::{SecretContract, SecretContractForShadeProtocol};
use cosmwasm_std::{to_binary, Binary, Coin, CosmosMsg, HumanAddr, StdResult, Uint128, WasmMsg};
use schemars::JsonSchema;
use secret_toolkit::utils::space_pad;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Receive {
        from: HumanAddr,
        msg: Option<Binary>,
        amount: Uint128,
    },
    FinalizeRoute {},
    RegisterTokens {
        tokens: Vec<SecretContract>,
    },
    RescueTokens {
        amount: Uint128,
        denom: Option<String>,
        token: Option<SecretContract>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
}

// Adapted from https://github.com/scrtlabs/secret-toolkit/blob/master/packages/snip20/src/handle.rs
// as that version only wraps scrt.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Snip20 {
    Deposit { padding: Option<String> },
}
impl Snip20 {
    pub fn to_cosmos_msg(
        &self,
        mut block_size: usize,
        callback_code_hash: String,
        contract_addr: HumanAddr,
        coin: Option<Coin>,
    ) -> StdResult<CosmosMsg> {
        // can not have block size of 0
        if block_size == 0 {
            block_size = 1;
        }
        let mut msg = to_binary(self)?;
        space_pad(&mut msg.0, block_size);
        let mut send = Vec::new();
        if let Some(coin_unwrapped) = coin {
            send.push(coin_unwrapped);
        }
        let execute = WasmMsg::Execute {
            contract_addr,
            callback_code_hash,
            msg,
            send,
        };
        Ok(execute.into())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Snip20Swap {
    Swap {
        expected_return: Option<Uint128>,
        to: Option<HumanAddr>,
    },
}

// https://github.com/securesecrets/shadeswap/blob/main/contracts/router/src/contract.rs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShadeProtocol {
    SwapTokensForExact {
        path: Vec<SecretContractForShadeProtocol>,
    },
}
