use crate::authorize::authorize;
use crate::constants::*;
use crate::msg::{
    space_pad, HandleAnswer, HandleMsg, InitMsg, QueryAnswer, QueryMsg, ReceiveAnswer, ReceiveMsg,
    ResponseStatus::Success,
};
use crate::rand::sha_256;
use crate::state::{read_viewing_key, write_viewing_key, Authentication, Config, User};
use crate::viewing_key::{ViewingKey, VIEWING_KEY_SIZE};
use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, QueryResult, StdError, StdResult, Storage, Uint128,
};
use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let config: Config = Config {
        buttcoin: msg.buttcoin,
        butt_lode: msg.butt_lode,
        prng_seed: sha_256(&msg.prng_seed.0).to_vec(),
    };
    config_store.store(CONFIG_KEY, &config)?;

    let messages = vec![snip20::register_receive_msg(
        env.contract_code_hash.clone(),
        None,
        RESPONSE_BLOCK_SIZE,
        config.buttcoin.contract_hash,
        config.buttcoin.address,
    )?];

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    let response = match msg {
        HandleMsg::CreateViewingKey { entropy, .. } => try_create_key(deps, env, entropy),
        HandleMsg::Receive {
            from, amount, msg, ..
        } => receive(deps, env, from, amount, msg),
        HandleMsg::SetViewingKey { key, .. } => try_set_key(deps, env, key),
    };

    pad_response(response)
}

pub fn query<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, msg: QueryMsg) -> QueryResult {
    match msg {
        _ => viewing_keys_queries(deps, msg),
    }
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

fn query_hints<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
) -> StdResult<Binary> {
    let users_store = TypedStore::<User, S>::attach(&deps.storage);
    let user = users_store.load(address.0.as_bytes()).unwrap_or(User {
        authentications: vec![],
        next_authentication_id: 1,
    });
    let hints = user
        .authentications
        .into_iter()
        .map(|mut authentication| {
            if authentication.label.chars().nth(0).is_some() {
                authentication.label = authentication.label.chars().nth(0).unwrap().to_string();
                authentication.username =
                    authentication.username.chars().nth(0).unwrap().to_string();
                authentication.password =
                    authentication.password.chars().nth(0).unwrap().to_string();
                authentication.notes = authentication.notes.chars().nth(0).unwrap().to_string();
            }
            authentication
        })
        .collect::<Vec<Authentication>>();
    let result = QueryAnswer::Hints { hints: hints };
    to_binary(&result)
}

fn receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: Uint128,
    msg: Binary,
) -> StdResult<HandleResponse> {
    let msg: ReceiveMsg = from_binary(&msg)?;
    match msg {
        ReceiveMsg::Create {
            label,
            username,
            password,
            notes,
        } => try_create(deps, env, from, amount, label, username, password, notes),
    }
}

fn try_create<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: Uint128,
    label: String,
    username: String,
    password: String,
    notes: String,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    // Ensure that the sent tokens are Buttcoin
    authorize(config.buttcoin.address.clone(), env.message.sender.clone())?;
    // Ensure that amount sent in is 1 Buttcoin
    if amount != Uint128(AMOUNT_FOR_TRANSACTION) {
        return Err(StdError::generic_err(format!(
            "Amount sent in: {}. Amount required {}.",
            amount,
            Uint128(AMOUNT_FOR_TRANSACTION)
        )));
    }

    let users_store = TypedStore::<User, S>::attach(&deps.storage);
    let mut user = users_store.load(from.0.as_bytes()).unwrap_or(User {
        authentications: vec![],
        next_authentication_id: 1,
    });
    user.authentications.push(Authentication {
        id: user.next_authentication_id,
        label: label,
        username: username,
        password: password,
        notes: notes,
    });
    user.next_authentication_id += 1;
    TypedStoreMut::<User, S>::attach(&mut deps.storage).store(from.0.as_bytes(), &user)?;

    Ok(HandleResponse {
        messages: vec![snip20::transfer_msg(
            config.butt_lode.address,
            Uint128(AMOUNT_FOR_TRANSACTION),
            None,
            RESPONSE_BLOCK_SIZE,
            config.buttcoin.contract_hash,
            config.buttcoin.address,
        )?],
        log: vec![],
        data: Some(to_binary(&ReceiveAnswer::Create { status: Success })?),
    })
}

fn try_set_key<S: Storage, A: Api, Q: Querier>(
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

fn try_create_key<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    entropy: String,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    let prng_seed = config.prng_seed;

    let key = ViewingKey::new(&env, &prng_seed, (&entropy).as_ref());

    let message_sender = deps.api.canonical_address(&env.message.sender)?;
    write_viewing_key(&mut deps.storage, &message_sender, &key);

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::CreateViewingKey { key })?),
    })
}

fn viewing_keys_queries<S: Storage, A: Api, Q: Querier>(
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
                QueryMsg::Hints { address, .. } => query_hints(deps, &address),
            };
        }
    }

    to_binary(&QueryAnswer::ViewingKeyError {
        msg: "Wrong viewing key for this address or viewing key not set".to_string(),
    })
}
