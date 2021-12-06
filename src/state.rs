use crate::msg::{Hop, Route};
use cosmwasm_std::{HumanAddr, StdResult, Storage};
use cosmwasm_storage::{ReadonlySingleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub code_hash: String,
}

static KEY_ROUTE_STATE: &[u8] = b"route_state";

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct RouteState {
    pub is_done: bool,
    pub current_hop: Option<Hop>,
    pub remaining_route: Route,
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

static KEY_TOKENS: &[u8] = b"tokens";

pub fn store_tokens<S: Storage>(storage: &mut S, data: &Vec<HumanAddr>) -> StdResult<()> {
    Singleton::new(storage, KEY_TOKENS).save(data)
}

pub fn read_tokens<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    ReadonlySingleton::new(storage, KEY_TOKENS).load()
}
