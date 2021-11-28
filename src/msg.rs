use crate::state::{Authentication, SecretContract};
use crate::viewing_key::ViewingKey;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitMsg {
    pub buttcoin: SecretContract,
    pub butt_lode: SecretContract,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Receive {
        sender: HumanAddr,
        from: HumanAddr,
        amount: Uint128,
        msg: Binary,
    },
    SetViewingKey {
        key: String,
        padding: Option<String>,
    },
    Show {
        id: usize,
    },
    UpdateAuthentication {
        id: usize,
        label: String,
        username: String,
        password: String,
        notes: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HandleAnswer {
    SetViewingKey { status: ResponseStatus },
    Show { authentication: Authentication },
    UpdateAuthentication { authentication: Authentication },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Hints { address: HumanAddr, key: String },
}

impl QueryMsg {
    pub fn get_validation_params(&self) -> (Vec<&HumanAddr>, ViewingKey) {
        match self {
            Self::Hints { address, key, .. } => (vec![address], ViewingKey(key.clone())),
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    Hints { hints: Vec<Authentication> },
    ViewingKeyError { msg: String },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    Create {
        label: String,
        username: String,
        password: String,
        notes: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
}

// Take a Vec<u8> and pad it up to a multiple of `block_size`, using spaces at the end.
pub fn space_pad(block_size: usize, message: &mut Vec<u8>) -> &mut Vec<u8> {
    let len = message.len();
    let surplus = len % block_size;
    if surplus == 0 {
        return message;
    }

    let missing = block_size - surplus;
    message.reserve(missing);
    message.extend(std::iter::repeat(b' ').take(missing));
    message
}
