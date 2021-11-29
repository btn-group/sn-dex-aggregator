use crate::authorize::authorize;
use crate::constants::*;
use crate::msg::{
    space_pad, HandleAnswer, HandleMsg, InitMsg, QueryAnswer, QueryMsg, ReceiveMsg,
    ResponseStatus::Success,
};
use crate::state::{read_viewing_key, write_viewing_key, Authentication, Config, User};
use crate::viewing_key::{ViewingKey, VIEWING_KEY_SIZE};
use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse,
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
        HandleMsg::Receive {
            from, amount, msg, ..
        } => receive(deps, env, from, amount, msg),
        HandleMsg::SetViewingKey { key, .. } => set_key(deps, env, key),
        HandleMsg::Show { id } => show(deps, env, id),
        HandleMsg::UpdateAuthentication {
            id,
            label,
            username,
            password,
            notes,
        } => update_authentication(deps, env, id, label, username, password, notes),
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
        hints: vec![],
        next_authentication_id: 0,
    });
    let result = QueryAnswer::Hints { hints: user.hints };
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
        } => create_authentication(deps, env, from, amount, label, username, password, notes),
    }
}

fn create_authentication<S: Storage, A: Api, Q: Querier>(
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
        hints: vec![],
        next_authentication_id: 0,
    });
    let authentication: Authentication = Authentication {
        id: user.next_authentication_id,
        label: label,
        username: username,
        password: password,
        notes: notes,
    };
    user.authentications.push(authentication.clone());
    user.hints.push(generate_hint_from_authentication(
        user.authentications[user.next_authentication_id].clone(),
    ));
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
        log: vec![
            log("action", "create"),
            log("id", authentication.id),
            log("label", authentication.label),
            log("username", authentication.username),
            log("password", authentication.password),
            log("notes", authentication.notes),
        ],
        data: None,
    })
}

fn generate_hint_from_authentication(authentication: Authentication) -> Authentication {
    let hint_username: String = if authentication.username.len() > 0 {
        authentication.username.chars().nth(0).unwrap().to_string()
    } else {
        "".to_string()
    };
    let hint_password: String = if authentication.password.len() > 0 {
        authentication.password.chars().nth(0).unwrap().to_string()
    } else {
        "".to_string()
    };
    let hint_notes: String = if authentication.notes.len() > 0 {
        authentication.notes.chars().nth(0).unwrap().to_string()
    } else {
        "".to_string()
    };
    Authentication {
        id: authentication.id,
        label: authentication.label,
        username: hint_username,
        password: hint_password,
        notes: hint_notes,
    }
}

fn set_key<S: Storage, A: Api, Q: Querier>(
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

fn show<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    id: usize,
) -> StdResult<HandleResponse> {
    // Find or initialize User locker
    let users_store = TypedStore::<User, S>::attach(&deps.storage);
    let user = users_store
        .load(env.message.sender.0.as_bytes())
        .unwrap_or(User {
            authentications: vec![],
            hints: vec![],
            next_authentication_id: 0,
        });
    if id >= user.next_authentication_id {
        return Err(StdError::generic_err("Authentication not found."));
    }

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::Show {
            authentication: user.authentications[id].clone(),
        })?),
    })
}

fn update_authentication<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    id: usize,
    label: String,
    username: String,
    password: String,
    notes: String,
) -> StdResult<HandleResponse> {
    let users_store = TypedStore::<User, S>::attach(&deps.storage);
    let mut user = users_store
        .load(env.message.sender.0.as_bytes())
        .unwrap_or(User {
            authentications: vec![],
            hints: vec![],
            next_authentication_id: 0,
        });
    if id >= user.next_authentication_id {
        return Err(StdError::generic_err("Authentication not found."));
    }
    user.authentications[id].label = label;
    user.authentications[id].username = username;
    user.authentications[id].password = password;
    user.authentications[id].notes = notes;
    user.hints[id] = generate_hint_from_authentication(user.authentications[id].clone());
    TypedStoreMut::<User, S>::attach(&mut deps.storage)
        .store(env.message.sender.0.as_bytes(), &user)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::UpdateAuthentication {
            authentication: user.authentications[id].clone(),
        })?),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SecretContract;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};

    // === HELPERS ===
    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let env = mock_env(mock_user_address(), &[]);
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            buttcoin: mock_buttcoin(),
            butt_lode: mock_butt_lode(),
        };
        (init(&mut deps, env.clone(), msg), deps)
    }

    fn mock_authentication() -> Authentication {
        Authentication {
            id: 0,
            label: "Park".to_string(),
            username: "Username".to_string(),
            password: "Password!!!".to_string(),
            notes: "dumb shit variant".to_string(),
        }
    }

    fn mock_buttcoin() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("token-address"),
            contract_hash: "token-contract-hash".to_string(),
        }
    }

    fn mock_butt_lode() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("token-address"),
            contract_hash: "token-contract-hash".to_string(),
        }
    }

    fn mock_user_address() -> HumanAddr {
        HumanAddr::from("gary")
    }

    // === TESTS ===
    #[test]
    fn test_create() {
        let (_init_result, mut deps) = init_helper();

        // when creating an authentication
        let create_authentication_message = ReceiveMsg::Create {
            label: mock_authentication().label,
            username: mock_authentication().username,
            password: mock_authentication().password,
            notes: mock_authentication().notes,
        };
        let receive_msg = HandleMsg::Receive {
            sender: mock_user_address(),
            from: mock_user_address(),
            amount: Uint128(AMOUNT_FOR_TRANSACTION),
            msg: to_binary(&create_authentication_message).unwrap(),
        };

        // = when user sends in a token that is not Buttcoin
        // = * it raises an error
        let handle_result = handle(
            &mut deps,
            mock_env(mock_user_address(), &[]),
            receive_msg.clone(),
        );
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );

        // = when user sends in Buttcoin
        // == when user sends in the wrong amount
        let receive_msg = HandleMsg::Receive {
            sender: mock_user_address(),
            from: mock_user_address(),
            amount: Uint128(AMOUNT_FOR_TRANSACTION + 555),
            msg: to_binary(&create_authentication_message).unwrap(),
        };
        // == * it raises an error
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            receive_msg,
        );
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err(format!(
                "Amount sent in: {}. Amount required {}.",
                AMOUNT_FOR_TRANSACTION + 555,
                Uint128(AMOUNT_FOR_TRANSACTION)
            ))
        );
        // == when user sends in the right amount
        let receive_msg = HandleMsg::Receive {
            sender: mock_user_address(),
            from: mock_user_address(),
            amount: Uint128(AMOUNT_FOR_TRANSACTION),
            msg: to_binary(&create_authentication_message).unwrap(),
        };
        // == * it sends the BUTT to the BUTT lode
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            receive_msg,
        );
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::transfer_msg(
                mock_butt_lode().address,
                Uint128(AMOUNT_FOR_TRANSACTION),
                None,
                RESPONSE_BLOCK_SIZE,
                mock_buttcoin().contract_hash,
                mock_buttcoin().address,
            )
            .unwrap()],
        );
        // == * it specifies the correct log details
        assert_eq!(
            handle_result_unwrapped.log,
            vec![
                log("action", "create"),
                log("id", mock_authentication().id),
                log("label", mock_authentication().label),
                log("username", mock_authentication().username),
                log("password", mock_authentication().password),
                log("notes", mock_authentication().notes),
            ],
        );
        // == * it creates the authentication for that user
        let show_msg = HandleMsg::Show { id: 0 };
        let handle_result_unwrapped =
            handle(&mut deps, mock_env(mock_user_address(), &[]), show_msg).unwrap();
        let handle_result_data: HandleAnswer =
            from_binary(&handle_result_unwrapped.data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&handle_result_data).unwrap(),
            to_binary(&HandleAnswer::Show {
                authentication: mock_authentication(),
            })
            .unwrap()
        );

        // == * it creates the hint for that user
        let set_viewing_key_msg = HandleMsg::SetViewingKey {
            key: "bibigo".to_string(),
            padding: None,
        };
        handle(
            &mut deps,
            mock_env(mock_user_address(), &[]),
            set_viewing_key_msg,
        )
        .unwrap();
        let query_result = query(
            &deps,
            QueryMsg::Hints {
                address: mock_user_address(),
                key: "bibigo".to_string(),
            },
        )
        .unwrap();
        let query_answer: QueryAnswer = from_binary(&query_result).unwrap();
        match query_answer {
            QueryAnswer::Hints { hints } => {
                assert_eq!(hints[0].id, 0);
                assert_eq!(hints[0].label, mock_authentication().label);
                assert_eq!(
                    hints[0].username,
                    mock_authentication()
                        .username
                        .chars()
                        .nth(0)
                        .unwrap()
                        .to_string()
                );
                assert_eq!(
                    hints[0].password,
                    mock_authentication()
                        .password
                        .chars()
                        .nth(0)
                        .unwrap()
                        .to_string()
                );
                assert_eq!(
                    hints[0].notes,
                    mock_authentication()
                        .notes
                        .chars()
                        .nth(0)
                        .unwrap()
                        .to_string()
                );
            }
            _ => {}
        }
        // == * it increases the next_authentication_id by 1
        let create_authentication_message = ReceiveMsg::Create {
            label: "Apricot".to_string(),
            username: "Seeds".to_string(),
            password: "Good?".to_string(),
            notes: "xxx".to_string(),
        };
        let receive_msg = HandleMsg::Receive {
            sender: mock_user_address(),
            from: mock_user_address(),
            amount: Uint128(AMOUNT_FOR_TRANSACTION),
            msg: to_binary(&create_authentication_message).unwrap(),
        };
        handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            receive_msg,
        )
        .unwrap();
        let show_msg = HandleMsg::Show { id: 1 };
        let handle_result_unwrapped =
            handle(&mut deps, mock_env(mock_user_address(), &[]), show_msg).unwrap();
        let handle_result_data: HandleAnswer =
            from_binary(&handle_result_unwrapped.data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&handle_result_data).unwrap(),
            to_binary(&HandleAnswer::Show {
                authentication: Authentication {
                    id: 1,
                    label: "Apricot".to_string(),
                    username: "Seeds".to_string(),
                    password: "Good?".to_string(),
                    notes: "xxx".to_string()
                },
            })
            .unwrap()
        );
    }

    #[test]
    fn test_update_authentication() {
        let (_init_result, mut deps) = init_helper();

        // when user has created an authentication
        let create_authentication_message = ReceiveMsg::Create {
            label: mock_authentication().label,
            username: mock_authentication().username,
            password: mock_authentication().password,
            notes: mock_authentication().notes,
        };
        let receive_msg = HandleMsg::Receive {
            sender: mock_user_address(),
            from: mock_user_address(),
            amount: Uint128(AMOUNT_FOR_TRANSACTION),
            msg: to_binary(&create_authentication_message).unwrap(),
        };
        handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            receive_msg,
        )
        .unwrap();

        // = when user tries to update an authentication that does not exist
        // = * it raises an error
        let update_msg = HandleMsg::UpdateAuthentication {
            id: 1,
            label: 'b'.to_string(),
            username: 'c'.to_string(),
            password: 'd'.to_string(),
            notes: 'e'.to_string(),
        };
        let handle_result = handle(&mut deps, mock_env(mock_user_address(), &[]), update_msg);
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Authentication not found.")
        );

        // = when user tries to update an authentication that does exist
        // = * it updates successfully and returns the authentication in the response
        let update_msg = HandleMsg::UpdateAuthentication {
            id: 0,
            label: "b123".to_string(),
            username: "c123".to_string(),
            password: "d123".to_string(),
            notes: "e123".to_string(),
        };
        let handle_result_unwrapped =
            handle(&mut deps, mock_env(mock_user_address(), &[]), update_msg).unwrap();
        let handle_result_data: HandleAnswer =
            from_binary(&handle_result_unwrapped.data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&handle_result_data).unwrap(),
            to_binary(&HandleAnswer::UpdateAuthentication {
                authentication: Authentication {
                    id: 0,
                    label: "b123".to_string(),
                    username: "c123".to_string(),
                    password: "d123".to_string(),
                    notes: "e123".to_string(),
                },
            })
            .unwrap()
        );

        let set_viewing_key_msg = HandleMsg::SetViewingKey {
            key: "bibigo".to_string(),
            padding: None,
        };
        handle(
            &mut deps,
            mock_env(mock_user_address(), &[]),
            set_viewing_key_msg,
        )
        .unwrap();
        let query_result = query(
            &deps,
            QueryMsg::Hints {
                address: mock_user_address(),
                key: "bibigo".to_string(),
            },
        )
        .unwrap();
        let query_answer: QueryAnswer = from_binary(&query_result).unwrap();
        match query_answer {
            QueryAnswer::Hints { hints } => {
                assert_eq!(hints[0].id, 0);
                assert_eq!(hints[0].label, "b123".to_string());
                assert_eq!(hints[0].username, "c".to_string());
                assert_eq!(hints[0].password, "d".to_string());
                assert_eq!(hints[0].notes, "e".to_string());
                assert_eq!(hints.len(), 1);
            }
            _ => {}
        }
    }

    #[test]
    fn test_handle_set_viewing_key() {
        let (init_result, mut deps) = init_helper();
        assert!(
            init_result.is_ok(),
            "Init failed: {}",
            init_result.err().unwrap()
        );

        // Set VK
        let handle_msg = HandleMsg::SetViewingKey {
            key: "hi lol".to_string(),
            padding: None,
        };
        let handle_result = handle(&mut deps, mock_env("bob", &[]), handle_msg);
        let unwrapped_result: HandleAnswer =
            from_binary(&handle_result.unwrap().data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&unwrapped_result).unwrap(),
            to_binary(&HandleAnswer::SetViewingKey { status: Success }).unwrap(),
        );

        // Set valid VK
        let actual_vk = ViewingKey("x".to_string().repeat(VIEWING_KEY_SIZE));
        let handle_msg = HandleMsg::SetViewingKey {
            key: actual_vk.0.clone(),
            padding: None,
        };
        let handle_result = handle(&mut deps, mock_env("bob", &[]), handle_msg);
        let unwrapped_result: HandleAnswer =
            from_binary(&handle_result.unwrap().data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&unwrapped_result).unwrap(),
            to_binary(&HandleAnswer::SetViewingKey { status: Success }).unwrap(),
        );
        let bob_canonical = deps
            .api
            .canonical_address(&HumanAddr("bob".to_string()))
            .unwrap();
        let saved_vk = read_viewing_key(&deps.storage, &bob_canonical).unwrap();
        assert!(actual_vk.check_viewing_key(&saved_vk));
    }
}
