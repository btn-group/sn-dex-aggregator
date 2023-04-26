use crate::authorize::{
    authorize, validate_received_from_an_allowed_address, validate_received_token,
    validate_user_is_the_receiver,
};
use crate::constants::{BLOCK_SIZE, CONFIG_KEY};
use crate::{
    msg::{HandleMsg, InitMsg, ShadeProtocol, Snip20, Snip20Swap},
    state::{
        delete_route_state, read_route_state, store_route_state, Config, Hop, Route, RouteState,
        SecretContract, Token,
    },
};
use cosmwasm_std::{
    from_binary, to_binary, Api, BankMsg, Binary, Coin, CosmosMsg, Env, Extern, HandleResponse,
    HumanAddr, InitResponse, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    _msg: InitMsg,
) -> StdResult<InitResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let config: Config = Config {
        admin: env.message.sender,
    };
    config_store.store(CONFIG_KEY, &config)?;

    Ok(InitResponse {
        messages: vec![],
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Receive {
            from,
            msg: Some(msg),
            amount,
        } => handle_first_hop(deps, &env, from, msg, amount),
        HandleMsg::Receive {
            from,
            msg: None,
            amount,
        } => handle_hop(deps, &env, from, amount),
        HandleMsg::FinalizeRoute {} => finalize_route(deps, &env),
        HandleMsg::RegisterTokens { tokens } => register_tokens(&env, tokens),
        HandleMsg::RescueTokens {
            amount,
            denom,
            token,
        } => rescue_tokens(deps, &env, amount, denom, token),
    }
}

// CONTNUE WITH THIS SO THAT WE CAN USE THIS ON
fn hop_messages(hop: Hop, amount: Uint128, env: &Env) -> StdResult<Vec<CosmosMsg>> {
    let mut msgs: Vec<CosmosMsg> = vec![];
    match hop.from_token {
        // first hop is a snip20
        Token::Snip20(SecretContract {
            address,
            contract_hash,
        }) => {
            // I also need to be able to handle shade protocol swap code
            if hop.shade_protocol_router_path.is_some() {
                // Shade Protocol Router
                // Just need the
                msgs.push(snip20::send_msg(
                    hop.smart_contract.unwrap().address,
                    amount,
                    // build swap msg for the next hop
                    Some(to_binary(&ShadeProtocol::SwapTokensForExact {
                        // set the recepient of the swap to be this contract (the router)
                        path: hop.shade_protocol_router_path.unwrap(),
                    })?),
                    None,
                    BLOCK_SIZE,
                    contract_hash,
                    address,
                )?);
            } else if hop.migrate_to_token.is_some() {
                // Migration
                // 1. Migrating
                msgs.push(snip20::send_msg(
                    hop.smart_contract.unwrap().address,
                    amount,
                    None,
                    None,
                    BLOCK_SIZE,
                    contract_hash,
                    address,
                )?);
                // 2. Continuing to next hop by sending the migrated token to self
                msgs.push(snip20::send_msg(
                    env.contract.address.clone(),
                    amount,
                    None,
                    None,
                    BLOCK_SIZE,
                    hop.migrate_to_token.clone().unwrap().contract_hash,
                    hop.migrate_to_token.unwrap().address,
                )?);
            } else if hop.redeem_denom.is_some() {
                // Redeen denom
                msgs.push(snip20::redeem_msg(
                    amount,
                    hop.redeem_denom.clone(),
                    None,
                    BLOCK_SIZE,
                    contract_hash,
                    address,
                )?);
                msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.clone(),
                    callback_code_hash: env.contract_code_hash.clone(),
                    msg: to_binary(&HandleMsg::Receive {
                        from: env.contract.address.clone(),
                        msg: None,
                        amount,
                    })
                    .unwrap(),
                    send: vec![Coin {
                        amount,
                        denom: hop.redeem_denom.unwrap(),
                    }],
                }))
            } else {
                // Standard
                msgs.push(snip20::send_msg(
                    hop.smart_contract.unwrap().address,
                    amount,
                    // build swap msg for the next hop
                    Some(to_binary(&Snip20Swap::Swap {
                        // set expected_return to None because we don't care about slippage mid-route
                        expected_return: None,
                        // set the recepient of the swap to be this contract (the router)
                        to: Some(env.contract.address.clone()),
                    })?),
                    None,
                    BLOCK_SIZE,
                    contract_hash,
                    address,
                )?);
            }
        }
        Token::Native(SecretContract {
            address,
            contract_hash,
        }) => {
            // DEPOSIT MSG
            msgs.push(Snip20::Deposit { padding: None }.to_cosmos_msg(
                BLOCK_SIZE,
                contract_hash.clone(),
                address.clone(),
                Some(Coin {
                    amount,
                    denom: hop.redeem_denom.unwrap(),
                }),
            )?);
            msgs.push(snip20::send_msg(
                env.contract.address.clone(),
                amount,
                None,
                None,
                BLOCK_SIZE,
                contract_hash,
                address,
            )?);
        }
    }

    Ok(msgs)
}

fn handle_first_hop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    from: HumanAddr,
    msg: Binary,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    // This is the first msg from the user, with the entire route details
    // 1. save the remaining route to state (e.g. if the route is X/Y -> Y/Z -> Z->W then save Y/Z -> Z/W to state)
    // 2. send `amount` X to pair X/Y
    // 3. call FinalizeRoute to make sure everything went ok, otherwise revert the tx

    // *** CHECKED
    let Route {
        mut hops,
        to,
        estimated_amount,
        minimum_acceptable_amount,
    } = from_binary(&msg)?;

    // *** CHECKED: SECOND HOP MUST EXIST AS LAST HOP CHECKS MIN ACCEPTABLE AMOUNT
    if hops.len() < 2 {
        return Err(StdError::generic_err("Route must be at least 2 hops."));
    }

    // *** CHECKED
    let first_hop: Hop = hops.pop_front().unwrap();

    // *** CHECKED
    validate_received_token(first_hop.from_token.clone(), amount, &env)?;

    // *** CHECKED
    validate_user_is_the_receiver(
        first_hop.from_token.clone(),
        from,
        to.clone(),
        env.message.sender.clone(),
    )?;

    // *** CHECKED
    store_route_state(
        &mut deps.storage,
        &RouteState {
            current_hop: Some(first_hop.clone()),
            remaining_route: Route {
                hops: hops.clone(), // hops was mutated earlier when we did `hops.pop_front()`
                estimated_amount,
                minimum_acceptable_amount,
                to,
            },
        },
    )?;

    // *** CHECKED
    let mut msgs = hop_messages(first_hop, amount, &env)?;
    msgs.push(
        // finalize the route at the end, to make sure the route was completed successfully
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.clone(),
            callback_code_hash: env.contract_code_hash.clone(),
            msg: to_binary(&HandleMsg::FinalizeRoute {})?,
            send: vec![],
        }),
    );

    Ok(HandleResponse {
        messages: msgs,
        log: vec![],
        data: None,
    })
}

fn handle_hop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    from: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    // This is a receive msg somewhere along the route
    // 1. load route from state (Y/Z -> Z/W)
    // 2. save the remaining route to state (Z/W)
    // 3. send `amount` Y to pair Y/Z

    // 1'. load route from state (Z/W)
    // 2'. this is the last hop so delete the entire route state
    // 3'. send `amount` Z to pair Z/W with recepient `to`
    match read_route_state(&deps.storage)? {
        Some(RouteState {
            current_hop,
            remaining_route:
                Route {
                    mut hops,
                    estimated_amount,
                    minimum_acceptable_amount,
                    to,
                },
        }) => {
            // *** CHECKED
            let next_hop: Hop = match hops.pop_front() {
                Some(next_hop) => next_hop,
                None => return Err(StdError::generic_err("Route must be at least 1 hop.")),
            };

            // *** CHECKED
            validate_received_token(next_hop.from_token.clone(), amount, env)?;

            // ### CHECKED
            validate_received_from_an_allowed_address(
                current_hop.clone().unwrap(),
                next_hop.clone(),
                env,
                from,
            )?;

            let mut messages = vec![];
            if hops.is_empty() {
                // last hop
                // 1. set is_done to true for FinalizeRoute
                // 2. set expected_return for the final swap
                // 3. set the recipient of the final swap to be the user
                if amount.lt(&minimum_acceptable_amount) {
                    return Err(StdError::generic_err(
                        "Operation fell short of minimum_acceptable_amount",
                    ));
                }
                // Send fee to appropriate person
                if amount > estimated_amount {
                    let excess: Uint128 = (amount - estimated_amount).unwrap();
                    match next_hop.clone().from_token {
                        Token::Snip20(SecretContract {
                            address,
                            contract_hash,
                        }) => {
                            messages.push(snip20::transfer_msg(
                                config.admin,
                                excess,
                                None,
                                BLOCK_SIZE,
                                contract_hash.clone(),
                                address.clone(),
                            )?);
                        }
                        Token::Native(_) => match current_hop {
                            Some(Hop {
                                ref redeem_denom, ..
                            }) => {
                                messages.push(CosmosMsg::Bank(BankMsg::Send {
                                    from_address: env.contract.address.clone(),
                                    to_address: config.admin,
                                    amount: vec![Coin {
                                        amount: excess,
                                        denom: redeem_denom.clone().unwrap(),
                                    }],
                                }));
                            }
                            None => todo!(),
                        },
                    };
                }
                // Send estimate amount to user
                match next_hop.clone().from_token {
                    Token::Snip20(SecretContract {
                        address,
                        contract_hash,
                    }) => {
                        messages.push(snip20::send_msg(
                            to.clone(),
                            estimated_amount,
                            None,
                            None,
                            BLOCK_SIZE,
                            contract_hash,
                            address,
                        )?);
                    }
                    Token::Native(_) => match current_hop {
                        Some(Hop { redeem_denom, .. }) => {
                            messages.push(CosmosMsg::Bank(BankMsg::Send {
                                from_address: env.contract.address.clone(),
                                to_address: to.clone(),
                                amount: vec![Coin {
                                    amount: estimated_amount,
                                    denom: redeem_denom.unwrap(),
                                }],
                            }));
                        }
                        None => todo!(),
                    },
                };
            } else {
                messages = hop_messages(next_hop.clone(), amount, &env)?;
            }

            // *** CHECKED
            let current_hop = Some(next_hop.clone());
            store_route_state(
                &mut deps.storage,
                &RouteState {
                    current_hop,
                    remaining_route: Route {
                        hops, // hops was mutated earlier when we did `hops.pop_front()`
                        estimated_amount,
                        minimum_acceptable_amount,
                        to,
                    },
                },
            )?;

            Ok(HandleResponse {
                messages,
                log: vec![],
                data: None,
            })
        }
        None => Err(StdError::generic_err("cannot find route")),
    }
}

fn finalize_route<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<HandleResponse> {
    match read_route_state(&deps.storage)? {
        Some(RouteState {
            remaining_route, ..
        }) => {
            // this function is called only by the route creation function
            // it is intended to always make sure that the route was completed successfully
            // otherwise we revert the transaction
            authorize(env.contract.address.clone(), env.message.sender.clone())?;
            if !remaining_route.hops.is_empty() {
                return Err(StdError::generic_err(format!(
                    "cannot finalize: route still contains hops: {:?}",
                    remaining_route
                )));
            }
            delete_route_state(&mut deps.storage);
            Ok(HandleResponse::default())
        }
        None => Err(StdError::generic_err("no route to finalize")),
    }
}

fn register_tokens(env: &Env, tokens: Vec<SecretContract>) -> StdResult<HandleResponse> {
    let mut messages = vec![];
    for token in tokens {
        let address = token.address;
        let contract_hash = token.contract_hash;
        messages.push(snip20::register_receive_msg(
            env.contract_code_hash.clone(),
            None,
            BLOCK_SIZE,
            contract_hash.clone(),
            address.clone(),
        )?);
    }

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

fn rescue_tokens<S: Storage, A: Api, Q: Querier>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SecretContractForShadeProtocol;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};
    use std::collections::VecDeque;

    // === HELPERS ===
    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let env = mock_env(mock_contract_initiator_address(), &[]);
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {};
        (init(&mut deps, env, msg), deps)
    }

    fn mock_button() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-button-address"),
            contract_hash: "mock-button-contract-hash".to_string(),
        }
    }

    fn mock_contract() -> SecretContract {
        let env = mock_env(mock_user_address(), &[]);
        SecretContract {
            address: env.contract.address,
            contract_hash: env.contract_code_hash,
        }
    }

    fn mock_contract_initiator_address() -> HumanAddr {
        HumanAddr::from("btn.group")
    }

    fn mock_denom() -> String {
        "uatom".to_string()
    }

    fn mock_pair_contract() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("pair-contract-address"),
            contract_hash: "pair-contract-contract-hash".to_string(),
        }
    }

    fn mock_pair_contract_two() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("pair-contract-two-address"),
            contract_hash: "pair-contract-two-hash".to_string(),
        }
    }

    fn mock_shade_protocol_router() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-shade-protocol-router-address"),
            contract_hash: "mock-shade-protocol-router-contract-hash".to_string(),
        }
    }

    fn mock_sscrt() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-sscrt-address"),
            contract_hash: "mock-sscrt-contract-hash".to_string(),
        }
    }

    fn mock_token() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-token-address"),
            contract_hash: "mock-token-contract-hash".to_string(),
        }
    }

    fn mock_token_native() -> Token {
        Token::Native(mock_sscrt())
    }

    fn mock_token_snip20() -> Token {
        Token::Snip20(mock_sscrt())
    }

    fn mock_user_address() -> HumanAddr {
        HumanAddr::from("gary")
    }

    // === HANDLE TESTS ===
    #[test]
    fn test_finalize_route() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env(mock_user_address(), &[]);

        // when route state does not exist
        // * it raises an error
        let handle_msg = HandleMsg::FinalizeRoute {};
        let handle_result = handle(&mut deps, env.clone(), handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("no route to finalize")
        );

        // when route state exists
        // = when there are hops
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_native(),
            redeem_denom: None,
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        let route_state: RouteState = RouteState {
            current_hop: Some(Hop {
                from_token: mock_token_native(),
                redeem_denom: None,
                smart_contract: Some(mock_pair_contract()),
                migrate_to_token: None,
                shade_protocol_router_path: None,
            }),
            remaining_route: Route {
                hops: hops,
                estimated_amount: Uint128(1_000_000),
                minimum_acceptable_amount: Uint128(1_000_000),
                to: mock_user_address(),
            },
        };
        store_route_state(&mut deps.storage, &route_state).unwrap();
        // == when it isn't called by the contract
        // == * it raises an error
        let handle_result = handle(&mut deps, env.clone(), handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );
        // == when it's called by the contract
        // == * it raises an error
        let handle_result = handle(
            &mut deps,
            mock_env(mock_contract().address, &[]),
            handle_msg.clone(),
        );
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err(format!(
                "cannot finalize: route still contains hops: {:?}",
                route_state.remaining_route
            ))
        );

        // = when there are no hops
        let hops: VecDeque<Hop> = VecDeque::new();
        let route_state: RouteState = RouteState {
            current_hop: Some(Hop {
                from_token: mock_token_native(),
                redeem_denom: None,
                smart_contract: Some(mock_pair_contract()),
                migrate_to_token: None,
                shade_protocol_router_path: None,
            }),
            remaining_route: Route {
                hops: hops,
                estimated_amount: Uint128(1_000_000),
                minimum_acceptable_amount: Uint128(1_000_000),
                to: mock_user_address(),
            },
        };
        store_route_state(&mut deps.storage, &route_state).unwrap();
        // == when it isn't called by the contract
        // == * it raises an error
        let handle_result = handle(&mut deps, env.clone(), handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );
        // == when it's called by the contract
        // == * it returns an Ok response
        handle(
            &mut deps,
            mock_env(mock_contract().address, &[]),
            handle_msg.clone(),
        )
        .unwrap();
        let handle_result = handle(
            &mut deps,
            mock_env(mock_contract().address, &[]),
            handle_msg,
        );
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("no route to finalize")
        );
    }

    #[test]
    fn test_handle_first_hop() {
        let (_init_result, mut deps) = init_helper();
        let minimum_acceptable_amount: Uint128 = Uint128(1_000_000);
        let estimated_amount: Uint128 = Uint128(10_000_000);
        let transaction_amount: Uint128 = minimum_acceptable_amount;
        let env = mock_env(
            mock_user_address(),
            &[Coin {
                denom: mock_denom(),
                amount: transaction_amount,
            }],
        );

        // when there is less than 2 hops
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_native(),
            redeem_denom: Some(mock_denom()),
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops.clone(),
                    to: mock_user_address(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, env.clone(), handle_msg);
        // * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Route must be at least 2 hops.")
        );

        // when there is 2 or more hops
        // = when the from_token for the first hop is a native token
        // == when the amount specified does match the amount sent in
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_pair_contract_two()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops.clone(),
                    to: mock_user_address(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount + transaction_amount,
        };
        let handle_result = handle(&mut deps, env.clone(), handle_msg);
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Received crypto type or amount is wrong.")
        );
        // == when the amount specified matches the amount sent in
        // == when the to does not match the sender
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops.clone(),
                    to: mock_pair_contract().address,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, env.clone(), handle_msg);
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );

        // == when the to matches the sender
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops.clone(),
                    to: mock_user_address(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount,
        };
        let handle_result_unwrapped = handle(&mut deps, env.clone(), handle_msg).unwrap();
        // == * it stores the route state
        let route_state: RouteState = read_route_state(&deps.storage).unwrap().unwrap();
        assert_eq!(route_state.current_hop, Some(hops.pop_front().unwrap()));
        assert_eq!(
            route_state.remaining_route,
            Route {
                hops,
                estimated_amount: estimated_amount,
                minimum_acceptable_amount: minimum_acceptable_amount,
                to: mock_user_address(),
            }
        );
        // == * it converts the native token to secret version
        // == * it sends coverted token to the aggregator to initiate the next hop
        // == * it finalizes the route
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                Snip20::Deposit { padding: None }
                    .to_cosmos_msg(
                        BLOCK_SIZE,
                        mock_sscrt().contract_hash,
                        mock_sscrt().address,
                        Some(Coin {
                            amount: transaction_amount,
                            denom: mock_denom(),
                        }),
                    )
                    .unwrap(),
                snip20::send_msg(
                    mock_contract().address,
                    transaction_amount,
                    None,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: mock_contract().address,
                    callback_code_hash: mock_contract().contract_hash.clone(),
                    msg: to_binary(&HandleMsg::FinalizeRoute {}).unwrap(),
                    send: vec![],
                }),
            ]
        );
        // = when the from_token for the first hop is a snip20
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        // == when the to does not match the from
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops.clone(),
                    to: mock_pair_contract().address,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );
        // == when the to matches the from
        // == * it sends the token to pair contract to swap
        // == * it finalizes route
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: Some(
                to_binary(&Route {
                    hops: hops,
                    to: mock_user_address(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                })
                .unwrap(),
            ),
            amount: transaction_amount,
        };
        let handle_result_unwrapped =
            handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg).unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::send_msg(
                    mock_pair_contract().address,
                    transaction_amount,
                    Some(
                        to_binary(&Snip20Swap::Swap {
                            expected_return: None,
                            to: Some(mock_contract().address),
                        })
                        .unwrap()
                    ),
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: mock_contract().address,
                    callback_code_hash: mock_contract().contract_hash.clone(),
                    msg: to_binary(&HandleMsg::FinalizeRoute {}).unwrap(),
                    send: vec![],
                }),
            ]
        );
    }

    #[test]
    fn test_handle_hop() {
        let (_init_result, mut deps) = init_helper();
        let minimum_acceptable_amount: Uint128 = Uint128(1_000_000);
        let estimated_amount: Uint128 = Uint128(10_000_000);
        let transaction_amount: Uint128 = minimum_acceptable_amount;

        // where there are no hops
        let mut hops: VecDeque<Hop> = VecDeque::new();
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: None,
                remaining_route: Route {
                    hops: hops.clone(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_button().address, &[]), handle_msg);
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Route must be at least 1 hop.")
        );

        // when there are hops
        // *** COMMENTED OUT WHILE DOING validate_received_token
        // = when expected token is a native token
        hops.push_back(Hop {
            from_token: mock_token_native(),
            redeem_denom: Some(mock_denom()),
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    redeem_denom: None,
                    smart_contract: Some(mock_button()),
                    migrate_to_token: None,
                    shade_protocol_router_path: None,
                }),
                remaining_route: Route {
                    hops,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        // // = * it raises an error
        // *** COMMENTED OUT WHILE DOING validate_received_token
        // let handle_msg = HandleMsg::Receive {
        //     from: mock_user_address(),
        //     msg: None,
        //     amount: transaction_amount,
        // };
        // let handle_result = handle(&mut deps, mock_env(mock_button().address, &[]), handle_msg);
        // assert_eq!(
        //     handle_result.unwrap_err(),
        //     StdError::generic_err("Native tokens can only be the input or output tokens.")
        // );
        // = when expected token is a snip20
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: Some(mock_denom()),
            smart_contract: Some(mock_pair_contract_two()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: Some(mock_denom()),
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    redeem_denom: None,
                    smart_contract: Some(mock_pair_contract()),
                    migrate_to_token: None,
                    shade_protocol_router_path: None,
                }),
                remaining_route: Route {
                    hops: hops.clone(),
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        // == when not from pair contract
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );
        // == when from this contract
        let handle_msg = HandleMsg::Receive {
            from: mock_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        // === when not expected token
        let handle_result = handle(&mut deps, mock_env(mock_user_address(), &[]), handle_msg);
        // === * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Received crypto type or amount is wrong.")
        );
        // === when sender is expected token
        // ==== when this is not the last hop
        // ==== * it swaps the token
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();

        // Redeen denom
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::redeem_msg(
                    transaction_amount,
                    Some(mock_denom()),
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address
                )
                .unwrap(),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: mock_contract().address.clone(),
                    callback_code_hash: mock_contract().contract_hash.clone(),
                    msg: to_binary(&HandleMsg::Receive {
                        from: mock_contract().address,
                        msg: None,
                        amount: transaction_amount,
                    })
                    .unwrap(),
                    send: vec![Coin {
                        amount: transaction_amount,
                        denom: mock_denom()
                    }],
                })
            ]
        );
        // ==== * it stores the updated route state
        let route_state = read_route_state(&deps.storage).unwrap().unwrap();
        assert_eq!(
            route_state.current_hop.unwrap(),
            Hop {
                from_token: mock_token_snip20(),
                redeem_denom: Some(mock_denom()),
                smart_contract: Some(mock_pair_contract_two()),
                migrate_to_token: None,
                shade_protocol_router_path: None,
            }
        );
        hops.pop_front();
        assert_eq!(
            route_state.remaining_route,
            Route {
                hops,
                estimated_amount: estimated_amount,
                minimum_acceptable_amount: minimum_acceptable_amount,
                to: mock_user_address(),
            },
        );
        // ==== when this is the last hop
        // ===== when the amount is less than the minimum_acceptable_amount
        let handle_msg = HandleMsg::Receive {
            from: mock_contract().address,
            msg: None,
            amount: (minimum_acceptable_amount - Uint128(1)).unwrap(),
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        // ===== * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Operation fell short of minimum_acceptable_amount")
        );
        // ====== when the amount is equal to or less than the estimated amount
        // ======= when the current hop does not have a smart contract associated with it
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    redeem_denom: Some(mock_denom()),
                    smart_contract: Some(mock_pair_contract()),
                    migrate_to_token: None,
                    shade_protocol_router_path: None,
                }),
                remaining_route: Route {
                    hops: hops,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        // ======= * it transfers the from token to the to value
        let handle_msg = HandleMsg::Receive {
            from: mock_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::send_msg(
                mock_user_address(),
                estimated_amount,
                None,
                None,
                BLOCK_SIZE,
                mock_sscrt().contract_hash,
                mock_sscrt().address,
            )
            .unwrap()]
        );
        // ======= when the current hop has a smart contract associated with it
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_button()),
            redeem_denom: None,
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    redeem_denom: None,
                    smart_contract: Some(mock_pair_contract()),
                    migrate_to_token: None,
                    shade_protocol_router_path: None,
                }),
                remaining_route: Route {
                    hops,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        // ===== when the amount is equal to or greater than the minimum_acceptable_amount
        // ====== when the amount is greater than the esimated amount
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    redeem_denom: None,
                    smart_contract: Some(mock_pair_contract_two()),
                    migrate_to_token: None,
                    shade_protocol_router_path: None,
                }),
                remaining_route: Route {
                    hops,
                    estimated_amount: estimated_amount,
                    minimum_acceptable_amount: minimum_acceptable_amount,
                    to: mock_user_address(),
                },
            },
        )
        .unwrap();
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract_two().address,
            msg: None,
            amount: estimated_amount + estimated_amount,
        };
        let handle_result = handle(&mut deps, mock_env(mock_sscrt().address, &[]), handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();
        // ======= * it transfers the positive slippage to contract initiator
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::transfer_msg(
                    mock_contract_initiator_address(),
                    estimated_amount,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                snip20::send_msg(
                    mock_user_address(),
                    estimated_amount,
                    None,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
            ]
        );
    }

    #[test]
    fn test_hop_messages() {
        let env = mock_env(mock_user_address(), &[]);
        let amount = Uint128(555);
        let shade_protocol_router_path: Option<Vec<SecretContractForShadeProtocol>> =
            Some(vec![SecretContractForShadeProtocol {
                addr: mock_pair_contract().address.to_string(),
                code_hash: mock_pair_contract().contract_hash,
            }]);
        let mut hop: Hop = Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_shade_protocol_router()),
            migrate_to_token: None,
            shade_protocol_router_path,
        };
        // when hop.from_token == Token::Snip20
        // = when shade_protocol_router_path is present
        // = * it sends the snip 20 to the hop smart contract with the SwapTokensForExact struct and path
        let mut messages: Vec<CosmosMsg> = hop_messages(hop.clone(), amount, &env).unwrap();
        assert_eq!(
            messages,
            vec![snip20::send_msg(
                hop.smart_contract.unwrap().address,
                amount,
                // build swap msg for the next hop
                Some(
                    to_binary(&ShadeProtocol::SwapTokensForExact {
                        // set the recepient of the swap to be this contract (the router)
                        path: hop.shade_protocol_router_path.unwrap(),
                    })
                    .unwrap()
                ),
                None,
                BLOCK_SIZE,
                mock_sscrt().contract_hash,
                mock_sscrt().address,
            )
            .unwrap()]
        );
        // = when migrate_to_token is present
        hop = Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_shade_protocol_router()),
            migrate_to_token: Some(mock_button()),
            shade_protocol_router_path: None,
        };
        // = * it sends the snip 20 to the hop smart contract and then it sends the migrate_to_token to itself
        messages = hop_messages(hop.clone(), amount, &env).unwrap();
        assert_eq!(
            messages,
            vec![
                snip20::send_msg(
                    hop.smart_contract.unwrap().address,
                    amount,
                    // build swap msg for the next hop
                    None,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                snip20::send_msg(
                    env.contract.address.clone(),
                    amount,
                    // build swap msg for the next hop
                    None,
                    None,
                    BLOCK_SIZE,
                    mock_button().contract_hash,
                    mock_button().address,
                )
                .unwrap(),
            ]
        );
        // = when redeem_denom is present
        // = * it unwraps the token and then sends the native token to itself with a receive message
        hop = Hop {
            from_token: mock_token_snip20(),
            redeem_denom: Some(mock_denom()),
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        };
        messages = hop_messages(hop, amount, &env).unwrap();
        assert_eq!(
            messages,
            vec![
                snip20::redeem_msg(
                    amount,
                    Some(mock_denom()),
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.clone(),
                    callback_code_hash: env.contract_code_hash.clone(),
                    msg: to_binary(&HandleMsg::Receive {
                        from: env.contract.address.clone(),
                        msg: None,
                        amount,
                    })
                    .unwrap(),
                    send: vec![Coin {
                        amount,
                        denom: mock_denom()
                    }],
                }),
            ]
        );
        // = when only smart_contract is present
        // = It sends a swap request to specificed smart contract
        hop = Hop {
            from_token: mock_token_snip20(),
            redeem_denom: None,
            smart_contract: Some(mock_pair_contract()),
            migrate_to_token: None,
            shade_protocol_router_path: None,
        };
        messages = hop_messages(hop, amount, &env).unwrap();
        assert_eq!(
            messages,
            vec![snip20::send_msg(
                mock_pair_contract().address,
                amount,
                Some(
                    to_binary(&Snip20Swap::Swap {
                        // set expected_return to None because we don't care about slippage mid-route
                        expected_return: None,
                        // set the recepient of the swap to be this contract (the router)
                        to: Some(env.contract.address.clone()),
                    })
                    .unwrap()
                ),
                None,
                BLOCK_SIZE,
                mock_sscrt().contract_hash,
                mock_sscrt().address,
            )
            .unwrap(),]
        );
        // when hop.from_token == Token::Native
        // = * it wraps the contract then sends it to itself
        hop = Hop {
            from_token: mock_token_native(),
            redeem_denom: Some(mock_denom()),
            smart_contract: None,
            migrate_to_token: None,
            shade_protocol_router_path: None,
        };
        messages = hop_messages(hop, amount, &env).unwrap();
        assert_eq!(
            messages,
            vec![
                Snip20::Deposit { padding: None }
                    .to_cosmos_msg(
                        BLOCK_SIZE,
                        mock_sscrt().contract_hash,
                        mock_sscrt().address,
                        Some(Coin {
                            amount,
                            denom: mock_denom(),
                        }),
                    )
                    .unwrap(),
                snip20::send_msg(
                    env.contract.address.clone(),
                    amount,
                    None,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap()
            ]
        );
    }

    #[test]
    fn test_register_tokens() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env(mock_user_address(), &[]);

        // When tokens are in the parameter
        let handle_msg = HandleMsg::RegisterTokens {
            tokens: vec![mock_button(), mock_token()],
        };
        let handle_result = handle(&mut deps, env.clone(), handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();
        // * it sends a message to register receive for the token and sets a viewing key
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::register_receive_msg(
                    mock_contract().contract_hash.clone(),
                    None,
                    BLOCK_SIZE,
                    mock_button().contract_hash,
                    mock_button().address,
                )
                .unwrap(),
                snip20::register_receive_msg(
                    mock_contract().contract_hash,
                    None,
                    BLOCK_SIZE,
                    mock_token().contract_hash,
                    mock_token().address,
                )
                .unwrap(),
            ]
        );
    }

    #[test]
    fn test_rescue_tokens() {
        let (_init_result, mut deps) = init_helper();
        let amount: Uint128 = Uint128(5);
        let mut handle_msg = HandleMsg::RescueTokens {
            amount,
            denom: Some(mock_denom()),
            token: Some(mock_button()),
        };
        // = when called by a non-admin
        // = * it raises an Unauthorized error
        let mut env: Env = mock_env(mock_user_address(), &[]);
        let handle_result = handle(&mut deps, env, handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );

        // = when called by the admin
        env = mock_env(mock_contract_initiator_address(), &[]);
        // == when only denom is specified
        handle_msg = HandleMsg::RescueTokens {
            amount,
            denom: Some(mock_denom()),
            token: None,
        };
        // === * it sends the amount specified of the coin of the denom to the admin
        let handle_result = handle(&mut deps, env.clone(), handle_msg.clone());
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![CosmosMsg::Bank(BankMsg::Send {
                from_address: env.contract.address,
                to_address: mock_contract_initiator_address(),
                amount: vec![Coin {
                    denom: mock_denom(),
                    amount
                }],
            })]
        );

        // == when only token is specified
        handle_msg = HandleMsg::RescueTokens {
            amount,
            denom: None,
            token: Some(mock_button()),
        };
        // == * it sends the amount specified of the token to the admin
        let handle_result = handle(
            &mut deps,
            mock_env(mock_contract_initiator_address(), &[]),
            handle_msg.clone(),
        );
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::transfer_msg(
                mock_contract_initiator_address(),
                amount,
                None,
                BLOCK_SIZE,
                mock_button().contract_hash,
                mock_button().address,
            )
            .unwrap()]
        );
    }
}
