use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const PREFIX_AUTHENTICATIONS: &[u8] = b"authentications";
const PREFIX_HINTS: &[u8] = b"hints";

// id will reflect the position in the array
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Authentication {
    pub id: u64,
    pub username: String,
    pub password: String,
    pub notes: String,
}
// id will reflect the position in the array
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Hint {
    pub id: u64,
    pub username: String,
    pub password: String,
    pub notes: String,
}

// // Storage functions:

// fn increment_tx_count<S: Storage>(store: &mut S) -> StdResult<u64> {
//     let mut config = Config::from_storage(store);
//     let id = config.tx_count() + 1;
//     config.set_tx_count(id)?;
//     Ok(id)
// }

// #[allow(clippy::too_many_arguments)] // We just need them
// pub fn store_transfer<S: Storage>(
//     store: &mut S,
//     owner: &CanonicalAddr,
//     sender: &CanonicalAddr,
//     receiver: &CanonicalAddr,
//     amount: Uint128,
//     denom: String,
//     memo: Option<String>,
//     block: &cosmwasm_std::BlockInfo,
// ) -> StdResult<()> {
//     let id = increment_tx_count(store)?;
//     let coins = Coin { denom, amount };
//     let transfer = StoredLegacyTransfer {
//         id,
//         from: owner.clone(),
//         sender: sender.clone(),
//         receiver: receiver.clone(),
//         coins,
//         memo,
//         block_time: block.time,
//         block_height: block.height,
//     };
//     let tx = StoredRichTx::from_stored_legacy_transfer(transfer.clone());

//     // Write to the owners history if it's different from the other two addresses
//     if owner != sender && owner != receiver {
//         // cosmwasm_std::debug_print("saving transaction history for owner");
//         append_tx(store, &tx, owner)?;
//         append_authentication(store, &transfer, owner)?;
//     }
//     // Write to the sender's history if it's different from the receiver
//     if sender != receiver {
//         // cosmwasm_std::debug_print("saving transaction history for sender");
//         append_tx(store, &tx, sender)?;
//         append_authentication(store, &transfer, sender)?;
//     }
//     // Always write to the recipient's history
//     // cosmwasm_std::debug_print("saving transaction history for receiver");
//     append_tx(store, &tx, receiver)?;
//     append_authentication(store, &transfer, receiver)?;

//     Ok(())
// }

// fn append_authentication<S: Storage>(
//     store: &mut S,
//     tx: &StoredLegacyTransfer,
//     for_address: &CanonicalAddr,
// ) -> StdResult<()> {
//     let mut store = PrefixedStorage::multilevel(&[PREFIX_AUTHENTICATIONS, for_address.as_slice()], store);
//     let mut store = AppendStoreMut::attach_or_create(&mut store)?;
//     store.push(tx)
// }

// pub fn get_authentications<A: Api, S: ReadonlyStorage>(
//     api: &A,
//     storage: &S,
//     for_address: &CanonicalAddr,
//     page: u32,
//     page_size: u32,
// ) -> StdResult<(Vec<Tx>, u64)> {
//     let store =
//         ReadonlyPrefixedStorage::multilevel(&[PREFIX_AUTHENTICATIONS, for_address.as_slice()], storage);

//     // Try to access the storage of authentications for the account.
//     // If it doesn't exist yet, return an empty list of authentications.
//     let store = AppendStore::<StoredLegacyTransfer, _, _>::attach(&store);
//     let store = if let Some(result) = store {
//         result?
//     } else {
//         return Ok((vec![], 0));
//     };

//     // Take `page_size` txs starting from the latest tx, potentially skipping `page * page_size`
//     // txs from the start.
//     let transfer_iter = store
//         .iter()
//         .rev()
//         .skip((page * page_size) as _)
//         .take(page_size as _);

//     // The `and_then` here flattens the `StdResult<StdResult<RichTx>>` to an `StdResult<RichTx>`
//     let authentications: StdResult<Vec<Tx>> = transfer_iter
//         .map(|tx| tx.map(|tx| tx.into_humanized(api)).and_then(|x| x))
//         .collect();
//     authentications.map(|txs| (txs, store.len() as u64))
// }
