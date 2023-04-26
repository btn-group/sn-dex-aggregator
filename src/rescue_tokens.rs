use crate::authorize::authorize;
use crate::constants::{BLOCK_SIZE, CONFIG_KEY};
use crate::state::{Config, SecretContract};
use cosmwasm_std::{
    Api, BankMsg, Coin, CosmosMsg, Env, Extern, HandleResponse, Querier, StdResult, Storage,
    Uint128,
};
use secret_toolkit::snip20;
use secret_toolkit::storage::TypedStore;

pub fn rescue_tokens<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    amount: Uint128,
    denom: Option<String>,
    token: Option<SecretContract>,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
    authorize(config.admin.clone(), env.message.sender.clone())?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Some(denom_unwrapped) = denom {
        let withdrawal_coin: Vec<Coin> = vec![Coin {
            amount,
            denom: denom_unwrapped,
        }];
        messages.push(CosmosMsg::Bank(BankMsg::Send {
            from_address: env.contract.address.clone(),
            to_address: config.admin.clone(),
            amount: withdrawal_coin,
        }));
    }

    if let Some(token_unwrapped) = token {
        messages.push(snip20::transfer_msg(
            config.admin,
            amount,
            None,
            BLOCK_SIZE,
            token_unwrapped.contract_hash,
            token_unwrapped.address,
        )?)
    }

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}
