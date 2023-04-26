use crate::state::{Hop, SecretContract, Token};
use cosmwasm_std::{Env, HumanAddr, StdError, StdResult, Uint128};

pub fn authorize(expected: HumanAddr, received: HumanAddr) -> StdResult<()> {
    if expected != received {
        return Err(StdError::Unauthorized { backtrace: None });
    }

    Ok(())
}

pub fn validate_received_from_an_allowed_address(
    current_hop: Hop,
    next_hop: Hop,
    env: &Env,
    from: HumanAddr,
) -> StdResult<()> {
    match next_hop.from_token {
        Token::Snip20(SecretContract { .. }) => {
            // 1. wrapped (redeem_denom present) - from must be from this contract
            // 2. from migration contract - from must be from this contract
            // 3. shade_protocol_router_path - from must be current_hop smart contract
            if current_hop.redeem_denom.is_some() {
                authorize(env.contract.address.clone(), from)?;
            } else if current_hop.migrate_to_token.is_some() {
                authorize(env.contract.address.clone(), from)?;
            } else if current_hop.smart_contract.is_some() {
                authorize(current_hop.smart_contract.unwrap().address, from)?;
            }
        }
        Token::Native(_) => {
            // Native token in handle_hop can only be from the contract
            authorize(env.message.sender.clone(), env.contract.address.clone())?;
        }
    };

    Ok(())
}

pub fn validate_received_token(token: Token, amount: Uint128, env: &Env) -> StdResult<()> {
    let token_valid: bool = match token {
        Token::Snip20(SecretContract {
            ref address,
            contract_hash: _,
        }) => env.message.sender == *address,
        Token::Native(_) => {
            env.message.sent_funds.len() == 1 && env.message.sent_funds[0].amount == amount
        }
    };

    if !token_valid {
        return Err(StdError::generic_err(
            "Received crypto type or amount is wrong.",
        ));
    }

    Ok(())
}

pub fn validate_user_is_the_receiver(
    token: Token,
    from: HumanAddr,
    to: HumanAddr,
    sender: HumanAddr,
) -> StdResult<()> {
    match token {
        Token::Snip20(SecretContract { .. }) => {
            authorize(from, to)?;
        }
        Token::Native(SecretContract { .. }) => {
            authorize(sender, to.clone())?;
        }
    }

    Ok(())
}
