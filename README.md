# Secret network DEX aggregator V1

## How it works
* User sends in a cryptocurreny, the actions that need to be taken (swaps, deposits, redeems), the minimum acceptable amount and the estimated amount.
* If the swaps don't end with the minimal acceptable ammount, the whole transaction is cancelled.

### Fees
* Other protocols take the positive slippage and send it to an address.
* We'd prefer an option where all positive slippage is swapped into BUTT and sent to BUTT lode, but can't figure out a good solution right now.
* For this version of the contract, if there's any positive slippage and the out token is BUTT, we'll send it to BUTT lode, but the rest we'll send to a team account.

### Algorithm example
1. ATOM -> sATOM via sATOM smart contract
2. sATOM -> SIENNA via trading pair smart contract on Sienna
3. SIENNA -> sWBTC ...
4. sWBTC -> BUTT via trading pair smart contract on Secret swap
5. BUTT -> sXMR ...
6. sXMR -> SEFI ...
7. SEFI -> sSCRT ...
8. sSCRT -> SCRT via sSCRT smart contract

## References
1. DEX aggregator: https://btn.group/secret_network/dex_aggregator
2. Secret contracts guide: https://github.com/enigmampc/secret-contracts-guide
3. 1inch BSC aggregator conract: https://bscscan.com/address/0x1111111254fb6c44bAC0beD2854e76F90643097d#code
4. AutoSwap aggregator contract: https://bscscan.com/address/0xbd6ed39bedf95517c45ddb3a35b647a462eed55d#code
