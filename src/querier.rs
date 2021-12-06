use cosmwasm_std::{
    Api, BalanceResponse, BankQuery, Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage,
    Uint128,
};
use secret_toolkit::snip20::balance_query;

pub fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_addr: &HumanAddr,
    denom: String,
) -> StdResult<Uint128> {
    // load price form the oracle
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: HumanAddr::from(account_addr),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

pub fn query_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    contract_hash: &String,
    account_addr: &HumanAddr,
    viewing_key: &String,
) -> StdResult<Uint128> {
    let msg = balance_query(
        &deps.querier,
        account_addr.clone(),
        viewing_key.clone(),
        1,
        contract_hash.clone(),
        contract_addr.clone(),
    )?;

    Ok(msg.amount)
}
