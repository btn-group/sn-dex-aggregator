use crate::state::{SecretContract, Token};
use cosmwasm_std::{Env, HumanAddr, StdError, StdResult, Uint128};

pub fn authorize(expected: HumanAddr, received: HumanAddr) -> StdResult<()> {
    if expected != received {
        return Err(StdError::Unauthorized { backtrace: None });
    }

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
