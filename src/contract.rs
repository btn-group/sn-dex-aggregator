use crate::constants::{BLOCK_SIZE, CONFIG_KEY};
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
    let mut output_msgs: Vec<CosmosMsg> = vec![];
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let config: Config = Config {
        buttcoin: msg.buttcoin,
        butt_lode: msg.butt_lode,
        initiator: env.message.sender.clone(),
        registered_tokens: vec![],
    };
    config_store.store(CONFIG_KEY, &config)?;
    if let Some(tokens) = msg.register_tokens {
        output_msgs.extend(register_tokens(deps, &env, tokens)?);
    }

    Ok(InitResponse {
        messages: output_msgs,
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
            from: _,
            msg: Some(msg),
            amount,
        } => handle_first_hop(deps, &env, msg, amount),
        HandleMsg::Receive {
            from,
            msg: None,
            amount,
        } => handle_hop(deps, &env, from, amount),
        HandleMsg::FinalizeRoute {} => finalize_route(deps, &env),
        HandleMsg::RegisterTokens { tokens } => {
            let output_msgs = register_tokens(deps, &env, tokens)?;

            Ok(HandleResponse {
                messages: output_msgs,
                log: vec![],
                data: None,
            })
        }
    }
}

fn handle_first_hop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
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
        return Err(StdError::generic_err("route must be at least 2 hops"));
    }

    // Only the first from token can be a native token
    for i in 1..(hops.len() - 1) {
        match hops[i].from_token {
            Token::Native => {
                return Err(StdError::generic_err(
                    "Native tokens can only be the input or output tokens.",
                ))
            }
            _ => continue,
        }
    }

    // unwrap is cool because `hops.len() >= 2`
    let first_hop: Hop = hops.pop_front().unwrap();
    let received_first_hop: bool = match first_hop.from_token {
        Token::Snip20(SecretContract {
            ref address,
            contract_hash: _,
        }) => env.message.sender == *address,
        Token::Native => {
            env.message.sent_funds.len() == 1 && env.message.sent_funds[0].amount == amount
        }
    };

    if !received_first_hop {
        return Err(StdError::generic_err("Wrong crypto received."));
    }

    store_route_state(
        &mut deps.storage,
        &RouteState {
            is_done: false,
            current_hop: Some(first_hop.clone()),
            remaining_route: Route {
                hops, // hops was mutated earlier when we did `hops.pop_front()`
                estimated_amount,
                minimum_acceptable_amount,
                to,
            },
        },
    )?;

    let mut msgs = vec![];

    match first_hop.from_token {
        Token::Snip20(SecretContract {
            address,
            contract_hash,
        }) => {
            // first hop is a snip20
            msgs.push(snip20::send_msg(
                first_hop.contract_address,
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
        Token::Native => {
            msgs.push(snip20::deposit_msg(
                amount,
                None,
                BLOCK_SIZE,
                first_hop.contract_code_hash.clone(),
                first_hop.contract_address.clone(),
            )?);
            msgs.push(snip20::send_msg(
                env.contract.address.clone(),
                amount,
                None,
                None,
                BLOCK_SIZE,
                first_hop.contract_code_hash,
                first_hop.contract_address,
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
            is_done: _,
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
                None => return Err(StdError::generic_err("route must be at least 1 hop")),
            };

            let (from_token_address, from_token_code_hash) = match next_hop.clone().from_token {
                Token::Snip20(SecretContract {
                    address,
                    contract_hash,
                }) => (address, contract_hash),
                Token::Native => {
                    return Err(StdError::generic_err(
                        "Native tokens can only be the input or output tokens.",
                    ));
                }
            };

            // Need to fix this so that if the previous hop involved a native token
            // being swapped, the from should be the contract
            // I don't really see why this is a big deal or why it needs to be checked, as long as the last user gets their
            let from_pair_of_current_hop = match current_hop {
                Some(Hop {
                    from_token: _,
                    contract_code_hash: _,
                    ref contract_address,
                    interaction_type: _,
                }) => *contract_address == from,
                None => false,
            };

            if env.message.sender != from_token_address || !from_pair_of_current_hop {
                return Err(StdError::generic_err(
                    "route can only be called by receiving the token of the next hop from the previous pair",
                ));
            }

            let mut is_done = false;
            let mut msgs = vec![];
            let mut current_hop = Some(next_hop.clone());
            if hops.len() == 0 {
                // last hop
                // 1. set is_done to true for FinalizeRoute
                // 2. set expected_return for the final swap
                // 3. set the recipient of the final swap to be the user
                is_done = true;
                current_hop = None;
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

                if next_hop.interaction_type == "redeem" {
                    let exchange_rate = snip20::exchange_rate_query(
                        &deps.querier,
                        BLOCK_SIZE,
                        from_token_code_hash.clone(),
                        from_token_address.clone(),
                    )?;
                    let denom = exchange_rate.denom;
                    msgs.push(snip20::redeem_msg(
                        amount,
                        Some(denom.clone()),
                        None,
                        BLOCK_SIZE,
                        from_token_code_hash,
                        from_token_address,
                    )?);
                    let withdrawal_coins: Vec<Coin> = vec![Coin {
                        denom: denom,
                        amount,
                    }];
                    msgs.push(CosmosMsg::Bank(BankMsg::Send {
                        from_address: env.contract.address.clone(),
                        to_address: to.clone(),
                        amount: withdrawal_coins,
                    }))
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
                    next_hop.clone().contract_address,
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
                    is_done,
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
            is_done,
            current_hop,
            remaining_route,
        }) => {
            // this function is called only by the route creation function
            // it is intended to always make sure that the route was completed successfully
            // otherwise we revert the transaction

            if env.contract.address != env.message.sender {
                return Err(StdError::unauthorized());
            }
            if !is_done {
                return Err(StdError::generic_err(format!(
                    "cannot finalize: route is not done: {:?}",
                    remaining_route
                )));
            }
            if remaining_route.hops.len() != 0 {
                return Err(StdError::generic_err(format!(
                    "cannot finalize: route still contains hops: {:?}",
                    remaining_route
                )));
            }
            if current_hop != None {
                return Err(StdError::generic_err(format!(
                    "cannot finalize: route still processing hops: {:?}",
                    remaining_route
                )));
            }

            delete_route_state(&mut deps.storage);
            Ok(HandleResponse::default())
        }
        None => Err(StdError::generic_err("no route to finalize")),
    }
}

fn register_tokens<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    tokens: Vec<SecretContract>,
) -> StdResult<Vec<CosmosMsg>> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;
    let mut output_msgs = vec![];

    for token in tokens {
        let address = token.address;
        let contract_hash = token.contract_hash;

        if config.registered_tokens.contains(&address) {
            continue;
        }
        config.registered_tokens.push(address.clone());

        output_msgs.push(snip20::register_receive_msg(
            env.contract_code_hash.clone(),
            None,
            BLOCK_SIZE,
            contract_hash.clone(),
            address.clone(),
        )?);
        output_msgs.push(snip20::set_viewing_key_msg(
            "DoTheRightThing.".into(),
            None,
            BLOCK_SIZE,
            contract_hash.clone(),
            address.clone(),
        )?);
    }
    config_store.store(CONFIG_KEY, &config)?;
    return Ok(output_msgs);
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
            register_tokens: None,
        };
        (init(&mut deps, env.clone(), msg), deps)
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

    // === QUERY TESTS ===

    #[test]
    fn test_query_config() {
        let (_init_result, deps) = init_helper();
        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let query_result = query(&deps, QueryMsg::Config {}).unwrap();
        let query_answer_config: Config = from_binary(&query_result).unwrap();
        assert_eq!(query_answer_config, config);
    }
}
