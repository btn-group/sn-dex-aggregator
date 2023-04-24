use crate::constants::KEY_ROUTE_STATE;
use cosmwasm_std::{HumanAddr, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    pub button: SecretContract,
    pub butt_lode: SecretContract,
    pub initiator: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Hop {
    pub from_token: Token,
    pub smart_contract: Option<SecretContract>,
    pub redeem_denom: Option<String>,
    pub migrate_to_token: Option<SecretContract>,
    pub shade_protocol_router_path: Option<Vec<SecretContractForShadeProtocol>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Route {
    pub hops: VecDeque<Hop>,
    pub estimated_amount: Uint128,
    pub minimum_acceptable_amount: Uint128,
    pub to: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct RouteState {
    pub current_hop: Option<Hop>,
    pub remaining_route: Route,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContractForShadeProtocol {
    pub addr: String,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    Snip20(SecretContract),
    Native(SecretContract),
}

pub fn store_route_state<S: Storage>(storage: &mut S, data: &RouteState) -> StdResult<()> {
    singleton(storage, KEY_ROUTE_STATE).save(data)
}

pub fn read_route_state<S: Storage>(storage: &S) -> StdResult<Option<RouteState>> {
    singleton_read(storage, KEY_ROUTE_STATE).may_load()
}

pub fn delete_route_state<S: Storage>(storage: &mut S) {
    singleton::<S, Option<RouteState>>(storage, KEY_ROUTE_STATE).remove();
}
