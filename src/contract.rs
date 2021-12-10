use crate::authorize::authorize;
use crate::constants::{BLOCK_SIZE, CONFIG_KEY, VIEWING_KEY};
use crate::{
    msg::{HandleMsg, InitMsg, QueryMsg, Snip20Swap},
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
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let config: Config = Config {
        buttcoin: msg.buttcoin,
        butt_lode: msg.butt_lode,
        initiator: env.message.sender,
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
    }
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

    let Route {
        mut hops,
        to,
        estimated_amount,
        minimum_acceptable_amount,
    } = from_binary(&msg)?;

    if hops.len() < 2 {
        return Err(StdError::generic_err("Route must be at least 2 hops."));
    }

    // unwrap is cool because `hops.len() >= 2`
    let first_hop: Hop = hops.pop_front().unwrap();
    let received_first_hop: bool = match first_hop.from_token {
        Token::Snip20(SecretContract {
            ref address,
            contract_hash: _,
        }) => env.message.sender == *address,
        Token::Native(_) => {
            env.message.sent_funds.len() == 1 && env.message.sent_funds[0].amount == amount
        }
    };

    if !received_first_hop {
        return Err(StdError::generic_err(
            "Received crypto type or amount is wrong.",
        ));
    }

    store_route_state(
        &mut deps.storage,
        &RouteState {
            current_hop: Some(first_hop.clone()),
            remaining_route: Route {
                hops, // hops was mutated earlier when we did `hops.pop_front()`
                estimated_amount,
                minimum_acceptable_amount,
                to: to.clone(),
            },
        },
    )?;

    let mut msgs = vec![];

    match first_hop.from_token {
        Token::Snip20(SecretContract {
            address,
            contract_hash,
        }) => {
            authorize(from, to)?;

            // first hop is a snip20
            msgs.push(snip20::send_msg(
                first_hop.smart_contract.unwrap().address,
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
        Token::Native(SecretContract {
            address,
            contract_hash,
        }) => {
            authorize(env.message.sender.clone(), to)?;

            msgs.push(snip20::deposit_msg(
                amount,
                None,
                BLOCK_SIZE,
                contract_hash.clone(),
                address.clone(),
            )?);
            msgs.push(snip20::send_msg(
                first_hop.smart_contract.unwrap().address,
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
    mut amount: Uint128,
) -> StdResult<HandleResponse> {
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
            let next_hop: Hop = match hops.pop_front() {
                Some(next_hop) => next_hop,
                None => return Err(StdError::generic_err("Route must be at least 1 hop.")),
            };
            let (from_token_address, from_token_code_hash) = match next_hop.clone().from_token {
                Token::Snip20(SecretContract {
                    address,
                    contract_hash,
                }) => (address, contract_hash),
                Token::Native(_) => {
                    return Err(StdError::generic_err(
                        "Native tokens can only be the input or output tokens.",
                    ));
                }
            };
            let from_pair_of_current_hop = match current_hop {
                Some(Hop {
                    from_token: _,
                    smart_contract,
                }) => smart_contract.unwrap().address == from,
                None => false,
            };
            if env.message.sender != from_token_address || !from_pair_of_current_hop {
                return Err(StdError::generic_err(
                    "Route can only be called by receiving the token of the next hop from the previous pair.",
                ));
            }

            let mut msgs = vec![];
            let current_hop = Some(next_hop.clone());
            if hops.len() == 0 {
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
                let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
                if amount > estimated_amount {
                    let fee_recipient = if from_token_address == config.buttcoin.address {
                        config.butt_lode.address
                    } else {
                        config.initiator
                    };
                    msgs.push(snip20::transfer_msg(
                        fee_recipient,
                        (amount - estimated_amount).unwrap(),
                        None,
                        BLOCK_SIZE,
                        from_token_code_hash.clone(),
                        from_token_address.clone(),
                    )?);
                    amount = estimated_amount
                }
                if next_hop.smart_contract.is_some() {
                    let smart_contract: SecretContract = next_hop.smart_contract.unwrap();
                    let denom: String = query_crypto_denom(deps, smart_contract.clone())?;
                    msgs.push(snip20::redeem_msg(
                        amount,
                        Some(denom.clone()),
                        None,
                        BLOCK_SIZE,
                        smart_contract.contract_hash,
                        smart_contract.address,
                    )?);
                    let withdrawal_coins: Vec<Coin> = vec![Coin { denom, amount }];
                    msgs.push(CosmosMsg::Bank(BankMsg::Send {
                        from_address: env.contract.address.clone(),
                        to_address: to.clone(),
                        amount: withdrawal_coins,
                    }));
                } else {
                    msgs.push(snip20::transfer_msg(
                        to.clone(),
                        amount,
                        None,
                        BLOCK_SIZE,
                        from_token_code_hash,
                        from_token_address,
                    )?);
                }
            } else {
                // not last hop
                // 1. set expected_return to None because we don't care about slippage mid-route
                // 2. set the recipient of the swap to be this contract (the router)
                msgs.push(snip20::send_msg(
                    next_hop.smart_contract.unwrap().address,
                    amount,
                    Some(to_binary(&Snip20Swap::Swap {
                        expected_return: None,
                        to: Some(env.contract.address.clone()),
                    })?),
                    None,
                    BLOCK_SIZE,
                    from_token_code_hash,
                    from_token_address,
                )?);
            }
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
                messages: msgs,
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
            if remaining_route.hops.len() != 0 {
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

fn query_crypto_denom<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    token: SecretContract,
) -> StdResult<String> {
    if token.address == HumanAddr::from("mock-buttcoin-address") {
        Ok("ubutt".to_string())
    } else {
        let exchange_rate = snip20::exchange_rate_query(
            &deps.querier,
            BLOCK_SIZE,
            token.contract_hash,
            token.address,
        )?;
        Ok(exchange_rate.denom)
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
        messages.push(snip20::set_viewing_key_msg(
            VIEWING_KEY.into(),
            None,
            BLOCK_SIZE,
            contract_hash,
            address,
        )?);
    }

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
            Ok(to_binary(&config)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};
    use std::collections::VecDeque;

    // === HELPERS ===
    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let env = mock_env(mock_contract_initiator_address(), &[]);
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            buttcoin: mock_buttcoin(),
            butt_lode: mock_butt_lode(),
        };
        (init(&mut deps, env, msg), deps)
    }

    fn mock_buttcoin() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-buttcoin-address"),
            contract_hash: "mock-buttcoin-contract-hash".to_string(),
        }
    }

    fn mock_butt_lode() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("mock-buttlode-address"),
            contract_hash: "mock-buttlode-contract-hash".to_string(),
        }
    }

    fn mock_contract_address() -> HumanAddr {
        mock_env(mock_user_address(), &[]).contract.address
    }

    fn mock_contract_initiator_address() -> HumanAddr {
        HumanAddr::from("btn.group")
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

    // === QUERY TESTS ===
    #[test]
    fn test_query_config() {
        let (_init_result, deps) = init_helper();
        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let query_result = query(&deps, QueryMsg::Config {}).unwrap();
        let query_answer_config: Config = from_binary(&query_result).unwrap();
        assert_eq!(query_answer_config, config);
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
            smart_contract: Some(mock_pair_contract()),
        });
        let route_state: RouteState = RouteState {
            current_hop: Some(Hop {
                from_token: mock_token_native(),
                smart_contract: Some(mock_pair_contract()),
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
            mock_env(mock_contract_address(), &[]),
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
                smart_contract: Some(mock_pair_contract()),
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
            mock_env(mock_contract_address(), &[]),
            handle_msg.clone(),
        )
        .unwrap();
        let handle_result = handle(
            &mut deps,
            mock_env(mock_contract_address(), &[]),
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
                denom: "sscrt".to_string(),
                amount: transaction_amount,
            }],
        );

        // when there is less than 2 hops
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_native(),
            smart_contract: Some(mock_pair_contract()),
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
            smart_contract: Some(mock_pair_contract_two()),
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
                snip20::deposit_msg(
                    transaction_amount,
                    None,
                    BLOCK_SIZE,
                    mock_sscrt().contract_hash,
                    mock_sscrt().address,
                )
                .unwrap(),
                snip20::send_msg(
                    mock_pair_contract().address,
                    transaction_amount,
                    Some(
                        to_binary(&Snip20Swap::Swap {
                            expected_return: None,
                            to: Some(mock_contract_address()),
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
                    contract_addr: mock_contract_address(),
                    callback_code_hash: env.contract_code_hash.clone(),
                    msg: to_binary(&HandleMsg::FinalizeRoute {}).unwrap(),
                    send: vec![],
                }),
            ]
        );
        // = when the from_token for the first hop is a snip20
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            smart_contract: Some(mock_pair_contract()),
        });
        hops.push_back(Hop {
            from_token: mock_token_snip20(),
            smart_contract: Some(mock_pair_contract()),
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
                            to: Some(mock_contract_address()),
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
                    contract_addr: mock_contract_address(),
                    callback_code_hash: env.contract_code_hash.clone(),
                    msg: to_binary(&HandleMsg::FinalizeRoute {}).unwrap(),
                    send: vec![],
                }),
            ]
        );
    }

    #[test]
    fn test_handle_hop() {
        let (_init_result, mut deps) = init_helper();
        let mut hops: VecDeque<Hop> = VecDeque::new();
        let minimum_acceptable_amount: Uint128 = Uint128(1_000_000);
        let estimated_amount: Uint128 = Uint128(10_000_000);
        let transaction_amount: Uint128 = minimum_acceptable_amount;
        // where there are no hops
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract()),
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
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            handle_msg,
        );
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Route must be at least 1 hop.")
        );

        // when there are hops
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_butt_lode()),
            smart_contract: Some(mock_buttcoin()),
        });

        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_buttcoin()),
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
        // = when expected token is a native token
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: mock_token_native(),
            smart_contract: Some(mock_pair_contract()),
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_buttcoin()),
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
        // = * it raises an error
        let handle_msg = HandleMsg::Receive {
            from: mock_user_address(),
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            handle_msg,
        );
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Native tokens can only be the input or output tokens.")
        );
        // = when expected token is a snip20
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_butt_lode()),
            smart_contract: Some(mock_pair_contract()),
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract()),
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
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            handle_msg,
        );
        // == * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Route can only be called by receiving the token of the next hop from the previous pair.")
        );
        // == when from pair contract
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        // === when sender is not expected token
        let handle_result = handle(&mut deps, mock_env(mock_user_address(), &[]), handle_msg);
        // === * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Route can only be called by receiving the token of the next hop from the previous pair.")
        );
        // === when sender is expected token
        // ==== when this is not the last hop
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_butt_lode()),
            smart_contract: Some(mock_pair_contract()),
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract()),
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
        // ==== * it swaps the token
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_butt_lode().address, &[]),
            handle_msg,
        );
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::send_msg(
                mock_pair_contract().address,
                transaction_amount,
                Some(
                    to_binary(&Snip20Swap::Swap {
                        expected_return: None,
                        to: Some(mock_contract_address()),
                    })
                    .unwrap()
                ),
                None,
                BLOCK_SIZE,
                mock_butt_lode().contract_hash,
                mock_butt_lode().address,
            )
            .unwrap()]
        );
        // ==== * it stores the updated route state
        let route_state = read_route_state(&deps.storage).unwrap().unwrap();
        assert_eq!(
            route_state.current_hop.unwrap(),
            Hop {
                from_token: Token::Snip20(mock_butt_lode()),
                smart_contract: Some(mock_pair_contract()),
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
            from: mock_pair_contract().address,
            msg: None,
            amount: (minimum_acceptable_amount - Uint128(1)).unwrap(),
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_butt_lode().address, &[]),
            handle_msg,
        );
        // ===== * it raises an error
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::generic_err("Operation fell short of minimum_acceptable_amount")
        );
        // ====== when the amount is equal to or less than the estimated amount
        // ======= when the current hop does not have a smart contract associated with it
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_butt_lode()),
            smart_contract: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract()),
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
            from: mock_pair_contract().address,
            msg: None,
            amount: transaction_amount,
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_butt_lode().address, &[]),
            handle_msg,
        );
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::transfer_msg(
                mock_user_address(),
                transaction_amount,
                None,
                BLOCK_SIZE,
                mock_butt_lode().contract_hash,
                mock_butt_lode().address,
            )
            .unwrap()]
        );
        // ======= when the current hop has a smart contract associated with it
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_buttcoin()),
            smart_contract: Some(mock_buttcoin()),
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract()),
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
        // ======= when the from token is BUTT
        let handle_msg = HandleMsg::Receive {
            from: mock_pair_contract().address,
            msg: None,
            amount: estimated_amount + estimated_amount,
        };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            handle_msg,
        );
        let handle_result_unwrapped = handle_result.unwrap();
        // ======= * it transfers positive slippage to BUTT lode
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::transfer_msg(
                    mock_butt_lode().address,
                    estimated_amount,
                    None,
                    BLOCK_SIZE,
                    mock_buttcoin().contract_hash,
                    mock_buttcoin().address,
                )
                .unwrap(),
                snip20::redeem_msg(
                    estimated_amount,
                    Some("ubutt".to_string()),
                    None,
                    BLOCK_SIZE,
                    mock_buttcoin().contract_hash,
                    mock_buttcoin().address,
                )
                .unwrap(),
                CosmosMsg::Bank(BankMsg::Send {
                    from_address: mock_contract_address(),
                    to_address: mock_user_address(),
                    amount: vec![Coin {
                        denom: "ubutt".to_string(),
                        amount: estimated_amount
                    }],
                })
            ]
        );
        // ======= when the from token is not BUTT
        let mut hops: VecDeque<Hop> = VecDeque::new();
        hops.push_back(Hop {
            from_token: Token::Snip20(mock_token()),
            smart_contract: None,
        });
        store_route_state(
            &mut deps.storage,
            &RouteState {
                current_hop: Some(Hop {
                    from_token: mock_token_native(),
                    smart_contract: Some(mock_pair_contract_two()),
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
        let handle_result = handle(&mut deps, mock_env(mock_token().address, &[]), handle_msg);
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
                    mock_token().contract_hash,
                    mock_token().address,
                )
                .unwrap(),
                snip20::transfer_msg(
                    mock_user_address(),
                    estimated_amount,
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
    fn test_register_tokens() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env(mock_user_address(), &[]);

        // When tokens are in the parameter
        let handle_msg = HandleMsg::RegisterTokens {
            tokens: vec![mock_buttcoin(), mock_token()],
        };
        let handle_result = handle(&mut deps, env.clone(), handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();
        // * it sends a message to register receive for the token and sets a viewing key
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![
                snip20::register_receive_msg(
                    env.contract_code_hash.clone(),
                    None,
                    BLOCK_SIZE,
                    mock_buttcoin().contract_hash,
                    mock_buttcoin().address,
                )
                .unwrap(),
                snip20::set_viewing_key_msg(
                    VIEWING_KEY.into(),
                    None,
                    BLOCK_SIZE,
                    mock_buttcoin().contract_hash,
                    mock_buttcoin().address,
                )
                .unwrap(),
                snip20::register_receive_msg(
                    env.contract_code_hash,
                    None,
                    BLOCK_SIZE,
                    mock_token().contract_hash,
                    mock_token().address,
                )
                .unwrap(),
                snip20::set_viewing_key_msg(
                    VIEWING_KEY.into(),
                    None,
                    BLOCK_SIZE,
                    mock_token().contract_hash,
                    mock_token().address,
                )
                .unwrap()
            ]
        );
    }
}
