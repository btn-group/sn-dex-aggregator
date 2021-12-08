use crate::constants::KEY_ROUTE_STATE;
use cosmwasm_std::{HumanAddr, StdResult, Storage, Uint128};
use cosmwasm_storage::{ReadonlySingleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    pub buttcoin: SecretContract,
    pub butt_lode: SecretContract,
    pub initiator: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Hop {
    pub from_token: Token,
    pub contract_address: HumanAddr,
    pub contract_code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Route {
    pub hops: VecDeque<Hop>,
    pub estimated_amount: Uint128,
    pub minimum_acceptable_amount: Uint128,
    pub native_out_token: Option<bool>,
    pub to: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct RouteState {
    pub is_done: bool,
    pub current_hop: Option<Hop>,
    pub remaining_route: Route,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    Snip20(SecretContract),
    Native,
}

pub fn store_route_state<S: Storage>(storage: &mut S, data: &RouteState) -> StdResult<()> {
    Singleton::new(storage, KEY_ROUTE_STATE).save(data)
}

pub fn read_route_state<S: Storage>(storage: &S) -> StdResult<Option<RouteState>> {
    ReadonlySingleton::new(storage, KEY_ROUTE_STATE).may_load()
}

pub fn delete_route_state<S: Storage>(storage: &mut S) {
    storage.remove(KEY_ROUTE_STATE);
}
