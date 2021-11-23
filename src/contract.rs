use crate::msg::QueryWithPermit;
use crate::msg::{
    space_pad, HandleAnswer, HandleMsg, InitMsg, QueryAnswer, QueryMsg, ResponseStatus::Success,
};
use crate::rand::sha_256;
use crate::state::{read_viewing_key, write_viewing_key, Config, Constants, ReadonlyConfig};
use crate::viewing_key::{ViewingKey, VIEWING_KEY_SIZE};
/// This contract implements SNIP-20 standard:
/// https://github.com/SecretFoundation/SNIPs/blob/master/SNIP-20.md
use cosmwasm_std::{
    to_binary, Api, Binary, CanonicalAddr, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, QueryResult, ReadonlyStorage, StdError, StdResult, Storage,
};
use secret_toolkit::permit::{validate, Permission, Permit};

/// We make sure that responses from `handle` are padded to a multiple of this size.
pub const RESPONSE_BLOCK_SIZE: usize = 256;
pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let prng_seed_hashed = sha_256(&msg.prng_seed.0);

    let mut config = Config::from_storage(&mut deps.storage);
    config.set_constants(&Constants {
        contract_address: env.contract.address,
        prng_seed: prng_seed_hashed.to_vec(),
    })?;
    Ok(InitResponse::default())
}

fn pad_response(response: StdResult<HandleResponse>) -> StdResult<HandleResponse> {
    response.map(|mut response| {
        response.data = response.data.map(|mut data| {
            space_pad(RESPONSE_BLOCK_SIZE, &mut data.0);
            data
        });
        response
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    let response = match msg {
        HandleMsg::CreateViewingKey { entropy, .. } => try_create_key(deps, env, entropy),
        HandleMsg::SetViewingKey { key, .. } => try_set_key(deps, env, key),
    };

    pad_response(response)
}

pub fn query<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, msg: QueryMsg) -> QueryResult {
    match msg {
        QueryMsg::WithPermit { permit, query } => permit_queries(deps, permit, query),
        _ => viewing_keys_queries(deps, msg),
    }
}

fn permit_queries<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    permit: Permit,
    query: QueryWithPermit,
) -> Result<Binary, StdError> {
    // Validate permit content
    let token_address = ReadonlyConfig::from_storage(&deps.storage)
        .constants()?
        .contract_address;

    let account = validate(deps, PREFIX_REVOKED_PERMITS, &permit, token_address)?;

    // Permit validated! We can now execute the query.
    match query {
        QueryWithPermit::TransferHistory { page, page_size } => {
            if !permit.check_permission(&Permission::History) {
                return Err(StdError::generic_err(format!(
                    "No permission to query history, got permissions {:?}",
                    permit.params.permissions
                )));
            }

            query_transfers(deps, &account, page.unwrap_or(0), page_size)
        }
    }
}

pub fn viewing_keys_queries<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> QueryResult {
    let (addresses, key) = msg.get_validation_params();

    for address in addresses {
        let canonical_addr = deps.api.canonical_address(address)?;

        let expected_key = read_viewing_key(&deps.storage, &canonical_addr);

        if expected_key.is_none() {
            // Checking the key will take significant time. We don't want to exit immediately if it isn't set
            // in a way which will allow to time the command and determine if a viewing key doesn't exist
            key.check_viewing_key(&[0u8; VIEWING_KEY_SIZE]);
        } else if key.check_viewing_key(expected_key.unwrap().as_slice()) {
            return match msg {
                // Base
                QueryMsg::TransferHistory {
                    address,
                    page,
                    page_size,
                    ..
                } => query_transfers(deps, &address, page.unwrap_or(0), page_size),
            };
        }
    }

    to_binary(&QueryAnswer::ViewingKeyError {
        msg: "Wrong viewing key for this address or viewing key not set".to_string(),
    })
}

pub fn query_transfers<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account: &HumanAddr,
    page: u32,
    page_size: u32,
) -> StdResult<Binary> {
    // let address = deps.api.canonical_address(account)?;
    // let (txs, total) = get_transfers(&deps.api, &deps.storage, &address, page, page_size)?;

    let result = QueryAnswer::TransferHistory {
        txs: vec![],
        total: Some(123),
    };
    to_binary(&result)
}

pub fn try_set_key<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    key: String,
) -> StdResult<HandleResponse> {
    let vk = ViewingKey(key);

    let message_sender = deps.api.canonical_address(&env.message.sender)?;
    write_viewing_key(&mut deps.storage, &message_sender, &vk);

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::SetViewingKey { status: Success })?),
    })
}

pub fn try_create_key<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    entropy: String,
) -> StdResult<HandleResponse> {
    let constants = ReadonlyConfig::from_storage(&deps.storage).constants()?;
    let prng_seed = constants.prng_seed;

    let key = ViewingKey::new(&env, &prng_seed, (&entropy).as_ref());

    let message_sender = deps.api.canonical_address(&env.message.sender)?;
    write_viewing_key(&mut deps.storage, &message_sender, &key);

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::CreateViewingKey { key })?),
    })
}

pub fn get_transfers<A: Api, S: ReadonlyStorage>(
    api: &A,
    storage: &S,
    for_address: &CanonicalAddr,
    page: u32,
    page_size: u32,
) -> StdResult<HandleResponse> {
    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: None,
    })
    // let store =
    //     ReadonlyPrefixedStorage::multilevel(&[PREFIX_TRANSFERS, for_address.as_slice()], storage);

    // // Try to access the storage of transfers for the account.
    // // If it doesn't exist yet, return an empty list of transfers.
    // let store = AppendStore::<StoredLegacyTransfer, _, _>::attach(&store);
    // let store = if let Some(result) = store {
    //     result?
    // } else {
    //     return Ok((vec![], 0));
    // };

    // // Take `page_size` txs starting from the latest tx, potentially skipping `page * page_size`
    // // txs from the start.
    // let transfer_iter = store
    //     .iter()
    //     .rev()
    //     .skip((page * page_size) as _)
    //     .take(page_size as _);

    // // The `and_then` here flattens the `StdResult<StdResult<RichTx>>` to an `StdResult<RichTx>`
    // let transfers: StdResult<Vec<Tx>> = transfer_iter
    //     .map(|tx| tx.map(|tx| tx.into_humanized(api)).and_then(|x| x))
    //     .collect();
    // transfers.map(|txs| (txs, store.len() as u64))
}
