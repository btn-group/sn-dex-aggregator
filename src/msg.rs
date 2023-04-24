use crate::state::{SecretContract, SecretContractForShadeProtocol};
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub button: SecretContract,
    pub butt_lode: SecretContract,
}

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
