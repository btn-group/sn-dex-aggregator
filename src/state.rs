use crate::constants::PREFIX_VIEW_KEY;
use crate::viewing_key::ViewingKey;
use cosmwasm_std::{CanonicalAddr, HumanAddr, ReadonlyStorage, Storage};
use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// id will reflect the position in the array
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Authentication {
    pub id: u64,
    pub label: String,
    pub username: String,
    pub password: String,
    pub notes: String,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub authentications: Vec<Authentication>,
    pub next_authentication_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Config {
    pub buttcoin: SecretContract,
    pub prng_seed: Vec<u8>,
}

// Viewing Keys
pub fn write_viewing_key<S: Storage>(store: &mut S, owner: &CanonicalAddr, key: &ViewingKey) {
    let mut balance_store = PrefixedStorage::new(PREFIX_VIEW_KEY, store);
    balance_store.set(owner.as_slice(), &key.to_hashed());
}

pub fn read_viewing_key<S: Storage>(store: &S, owner: &CanonicalAddr) -> Option<Vec<u8>> {
    let balance_store = ReadonlyPrefixedStorage::new(PREFIX_VIEW_KEY, store);
    balance_store.get(owner.as_slice())
}
