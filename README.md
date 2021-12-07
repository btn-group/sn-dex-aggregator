# Secret network DEX aggregator

## How it works
* User selects the from and to cryptocurrencies
* User accepts a guanteedAmount and a minOutAmount
* The minOutAmount is the lowest amount the user is willing to accept for the swap. If the swaps don't end with at least this amount, the whole order is cancelled.
* The guaranteedAmount is the amount at which our UI has predicted is the most likely outcome. If the swaps end with an amount greater than this amount, that positive slippage is kept as fees.

### Fees
When the swapped asset is:
* BUTT: the positive slippage is sent to BUTT lode.
* Not BUTT and not a native token: it is left within the contract.
* A native token: the wrapped version of the native token is left within the contract.

The idea is that the left over token will hopefully be used in a future transaction, which will cause a positive slippage ending up at BUTT.

## References
1. DEX aggregator: https://btn.group/secret_network/dex_aggregator
2. Secret contracts guide: https://github.com/enigmampc/secret-contracts-guide
3. 1inch BSC aggregator conract: https://bscscan.com/address/0x1111111254fb6c44bAC0beD2854e76F90643097d#code
4. AutoSwap aggregator contract: https://bscscan.com/address/0xbd6ed39bedf95517c45ddb3a35b647a462eed55d#code

The one issue with this setup is that a user can manually enter the guaranteed amount and therefore never pay the fee...
I think a much better setup would be to provide an estimated amount, a minimum amount, and then charging a fee based on that... 
