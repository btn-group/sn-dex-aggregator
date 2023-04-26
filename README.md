# Secret network DEX aggregator V4

## How it works
* User sends in a cryptocurreny, the actions that need to be taken (swaps, deposits, redeems, migration), the minimum acceptable amount and the estimated amount.
* If the swaps don't end with the minimal acceptable amount, the whole transaction is cancelled.

### Fees
* Positive slippage is sent to the admin.

### Algorithm example (THIS IS OUT OF DATE)
1. ATOM -> sATOM via sATOM smart contract
2. sATOM -> SIENNA via trading pair smart contract on Sienna
3. SIENNA -> sWBTC ...
4. sWBTC -> BUTT via trading pair smart contract on Secret swap
5. BUTT -> sXMR ...
6. sXMR -> SEFI ...
7. SEFI -> sSCRT ...
8. sSCRT -> SCRT via sSCRT smart contract

## Testing locally examples (THIS IS OUT OF DATE)
```
# Run chain locally
docker run -it --rm -p 26657:26657 -p 26656:26656 -p 1337:1337 -v $(pwd):/root/code --name secretdev enigmampc/secret-network-sw-dev

# Access container via separate terminal window
docker exec -it secretdev /bin/bash

# cd into code folder
cd code

# Store contracts required for test
secretcli tx compute store button.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store secretswap-factory.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store secretswap-pair.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store secretswap-token.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store snip-20-reference-impl.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store sn-dex-aggregator.wasm.gz --from a --gas 3000000 -y --keyring-backend test

# Get the contract's id
secretcli query compute list-code

# Init Button
CODE_ID=1
INIT='{"name": "Button", "symbol": "BUTT", "decimals": 6, "initial_balances": [{"address": "secret1pl2r32z9n3e47950s6ej7mg7pfh7mxd003guum", "amount": "1000000000000000000"},{"address": "secret1jk7z6dhn9te3jh9d5sxcm8zu087zkz3tpxtmfe", "amount": "1000000000000000000"}], "prng_seed": "testing"}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "Button" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Set viewing key for Button
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"set_viewing_key": { "key": "testing" }}' --from a -y --keyring-backend test
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"set_viewing_key": { "key": "testing" }}' --from b -y --keyring-backend test

# Init Secret Swap factory
CODE_ID=3
INIT='{"pair_code_id": 4, "token_code_id": 5, "token_code_hash": "E690D84DB0C12ABED06A68C655F542C2653086D9F669617659FAB82713032C0D", "pair_code_hash": "2D5A05E72F3F6E6FD6F64CE6637B82AAF24608C4CD63A6EC10F73CB608C098FC", "prng_seed": "RG9UaGVSaWdodFRoaW5nLg=="}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "Secret swap factory" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Init SNIP-20 (sSCRT)
CODE_ID=6
INIT='{ "name": "sSCRT", "symbol": "SSCRT", "decimals": 6, "initial_balances": [{ "address": "secret1pl2r32z9n3e47950s6ej7mg7pfh7mxd003guum", "amount": "1000000000000000000" }, { "address": "secret1jk7z6dhn9te3jh9d5sxcm8zu087zkz3tpxtmfe", "amount": "1000000000000000000" }], "prng_seed": "RG9UaGVSaWdodFRoaW5nLg==", "config": { "public_total_supply": true, "enable_deposit": true, "enable_redeem": true, "enable_mint": true, "enable_burn": true } }'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "sSCRT" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Set viewing key for sSCRT
secretcli tx compute execute secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s '{"set_viewing_key": {"key": "DoTheRightThing.", "padding": "ThereWillBeButt."}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt
secretcli tx compute execute secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s '{"set_viewing_key": {"key": "DoTheRightThing.", "padding": "ThereWillBeButt."}}' --from b -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Create BUTT-sSCRT pair
secretcli tx compute execute secret1wgh6adn8geywx0v78zs9azrqtqdegufuafa8xa '{"create_pair": {"asset_infos": [{"token": {"contract_addr": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg", "token_code_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA", "viewing_key": "DoTheRightThing." }}, {"token": {"contract_addr": "secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s", "token_code_hash": "35F5DB2BC5CD56815D10C7A567D6827BECCB8EAF45BC3FA016930C4A8209EA69", "viewing_key": "DoTheRightThing."}}]}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Increase allowance to pair for the two tokens
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"increase_allowance":{"spender": "secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax", "amount": "1000000000000000000000000000"}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt
secretcli tx compute execute secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s '{"increase_allowance":{"spender": "secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax", "amount": "1000000000000000000000000000"}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Provide liquidity to pair
secretcli tx compute execute secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax '{"provide_liquidity":{"assets": [{"info":{"token":{"contract_addr": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg","token_code_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA","viewing_key": "DoTheRightThing."}},"amount": "1000000"},{"info":{"token":{"contract_addr": "secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s","token_code_hash": "35F5DB2BC5CD56815D10C7A567D6827BECCB8EAF45BC3FA016930C4A8209EA69","viewing_key": "DoTheRightThing."}},"amount": "1000000"}]}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Init DEX aggregator
CODE_ID=7
INIT='{}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "DEX aggregator 4 | btn.group" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Register tokens
secretcli tx compute execute secret15rrl3qjafxzlzguu5x29xh29pam35uetwpfnna '{"register_tokens":{"tokens": [{"address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg", "contract_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA"}, {"address": "secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s", "contract_hash": "35F5DB2BC5CD56815D10C7A567D6827BECCB8EAF45BC3FA016930C4A8209EA69"}]}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Query simulation / reverse simulation
secretcli query compute query secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax '{"simulation": {"offer_asset": {"info":{"token":{"contract_addr": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg","token_code_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA","viewing_key": "DoTheRightThing."}},"amount": "500"}}}'
=> {"return_amount":"499","spread_amount":"0","commission_amount":"1"}

secretcli query compute query secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax '{"reverse_simulation": {"ask_asset": {"info":{"token":{"contract_addr": "secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s","token_code_hash": "35F5DB2BC5CD56815D10C7A567D6827BECCB8EAF45BC3FA016930C4A8209EA69","viewing_key": "DoTheRightThing."}},"amount": "500"}}}'
=> {"offer_amount":"501","spread_amount":"0","commission_amount":"1"}

# Swap SCRT for BUTT with estimated amount the same as returned amount
secretcli tx compute execute secret15rrl3qjafxzlzguu5x29xh29pam35uetwpfnna '{"receive":{"from": "secret1pl2r32z9n3e47950s6ej7mg7pfh7mxd003guum", "amount": "500", "msg": "eyJob3BzIjogW3siZnJvbV90b2tlbiI6IHsibmF0aXZlIjogeyJhZGRyZXNzIjogInNlY3JldDF2YzV6Znd0MDhodTljY3RkZ2txazZqOXkwNXc2dHkweGE4ZzI2cyIsICJjb250cmFjdF9oYXNoIjogIjM1RjVEQjJCQzVDRDU2ODE1RDEwQzdBNTY3RDY4MjdCRUNDQjhFQUY0NUJDM0ZBMDE2OTMwQzRBODIwOUVBNjkifX0sICJzbWFydF9jb250cmFjdCI6IHsiYWRkcmVzcyI6ICJzZWNyZXQxbXkzanZsNnpzMm4yNzY0OHpuZ3FydHc4cGQyM25ya3JoMGY3YXgiLCAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIn19LCB7ImZyb21fdG9rZW4iOiB7InNuaXAyMCI6IHsiYWRkcmVzcyI6ICJzZWNyZXQxOHZkOGZwd3h6Y2s5M3Fsd2doYWo2YXJoNHA3YzVuODk3OHZzeWciLCAiY29udHJhY3RfaGFzaCI6ICI0Q0Q3RjY0QjlBREU2NTIwMEU1OTUyMTYyNjU5MzJBMEM3Njg5QzQ4MDRCRTdCNEE1RjhDRUJFRDI1MEJGN0VBIn19fV0sInRvIjoic2VjcmV0MXBsMnIzMno5bjNlNDc5NTBzNmVqN21nN3BmaDdteGQwMDNndXVtIiwiZXN0aW1hdGVkX2Ftb3VudCI6IjQ5OSIsICJtaW5pbXVtX2FjY2VwdGFibGVfYW1vdW50IjogIjQ5OSJ9"}}' --from a -y --keyring-backend test --gas 3000000 --amount "500uscrt" --gas-prices=3.0uscrt

# Query balance of BUTT
secretcli query compute query secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"balance": {"address": "secret1pl2r32z9n3e47950s6ej7mg7pfh7mxd003guum", "key": "testing"}}'

# Query simulation / reverse simulation
# !! This amount must be less than the amount of SCRT sent in the last swap as sSCRT was minted earlier on during init => 300 < 500
secretcli query compute query secret1my3jvl6zs2n27648zngqrtw8pd23nrkrh0f7ax '{"simulation": {"offer_asset": {"info":{"token":{"contract_addr": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg","token_code_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA","viewing_key": "DoTheRightThing."}},"amount": "300"}}}'
=> {"return_amount":"301","spread_amount":"0","commission_amount":"0"}

# Swap BUTT for SCRT
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"send": { "recipient": "secret15rrl3qjafxzlzguu5x29xh29pam35uetwpfnna", "amount": "300", "msg": "eyJob3BzIjogW3siZnJvbV90b2tlbiI6IHsic25pcDIwIjogeyJhZGRyZXNzIjogInNlY3JldDE4dmQ4ZnB3eHpjazkzcWx3Z2hhajZhcmg0cDdjNW44OTc4dnN5ZyIsICJjb250cmFjdF9oYXNoIjogIjRDRDdGNjRCOUFERTY1MjAwRTU5NTIxNjI2NTkzMkEwQzc2ODlDNDgwNEJFN0I0QTVGOENFQkVEMjUwQkY3RUEifX0sICJzbWFydF9jb250cmFjdCI6IHsiYWRkcmVzcyI6ICJzZWNyZXQxbXkzanZsNnpzMm4yNzY0OHpuZ3FydHc4cGQyM25ya3JoMGY3YXgiLCAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIn19LCB7ImZyb21fdG9rZW4iOiB7InNuaXAyMCI6IHsiYWRkcmVzcyI6ICJzZWNyZXQxdmM1emZ3dDA4aHU5Y2N0ZGdrcWs2ajl5MDV3NnR5MHhhOGcyNnMiLCAiY29udHJhY3RfaGFzaCI6ICIzNUY1REIyQkM1Q0Q1NjgxNUQxMEM3QTU2N0Q2ODI3QkVDQ0I4RUFGNDVCQzNGQTAxNjkzMEM0QTgyMDlFQTY5In19LCAic21hcnRfY29udHJhY3QiOiB7ImFkZHJlc3MiOiAic2VjcmV0MXZjNXpmd3QwOGh1OWNjdGRna3FrNmo5eTA1dzZ0eTB4YThnMjZzIiwgImNvbnRyYWN0X2hhc2giOiAiMzVGNURCMkJDNUNENTY4MTVEMTBDN0E1NjdENjgyN0JFQ0NCOEVBRjQ1QkMzRkEwMTY5MzBDNEE4MjA5RUE2OSJ9LCAicmVkZWVtX2Rlbm9tIjogInVzY3J0In1dLCJ0byI6InNlY3JldDFwbDJyMzJ6OW4zZTQ3OTUwczZlajdtZzdwZmg3bXhkMDAzZ3V1bSIsImVzdGltYXRlZF9hbW91bnQiOiIzMDEiLCAibWluaW11bV9hY2NlcHRhYmxlX2Ftb3VudCI6ICIzMDAifQ==" }}' --from a -y --keyring-backend test --gas 9000000 --gas-prices=3.0uscrt

# BUTT -> sSCRT -> BUTT
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"send": { "recipient": "secret15rrl3qjafxzlzguu5x29xh29pam35uetwpfnna", "amount": "300", "msg": "ewogICJob3BzIjogWwogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MTh2ZDhmcHd4emNrOTNxbHdnaGFqNmFyaDRwN2M1bjg5Nzh2c3lnIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjRDRDdGNjRCOUFERTY1MjAwRTU5NTIxNjI2NTkzMkEwQzc2ODlDNDgwNEJFN0I0QTVGOENFQkVEMjUwQkY3RUEiCiAgICAgICAgfQogICAgICB9LAogICAgICAic21hcnRfY29udHJhY3QiOiB7CiAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MW15M2p2bDZ6czJuMjc2NDh6bmdxcnR3OHBkMjNucmtyaDBmN2F4IiwKICAgICAgICAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIgogICAgICB9CiAgICB9LAogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MXZjNXpmd3QwOGh1OWNjdGRna3FrNmo5eTA1dzZ0eTB4YThnMjZzIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjM1RjVEQjJCQzVDRDU2ODE1RDEwQzdBNTY3RDY4MjdCRUNDQjhFQUY0NUJDM0ZBMDE2OTMwQzRBODIwOUVBNjkiCiAgICAgICAgfQogICAgICB9LAogICAgICAic21hcnRfY29udHJhY3QiOiB7CiAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MW15M2p2bDZ6czJuMjc2NDh6bmdxcnR3OHBkMjNucmtyaDBmN2F4IiwKICAgICAgICAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIgogICAgICB9CiAgICB9LAogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MTh2ZDhmcHd4emNrOTNxbHdnaGFqNmFyaDRwN2M1bjg5Nzh2c3lnIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjRDRDdGNjRCOUFERTY1MjAwRTU5NTIxNjI2NTkzMkEwQzc2ODlDNDgwNEJFN0I0QTVGOENFQkVEMjUwQkY3RUEiCiAgICAgICAgfQogICAgICB9CiAgICB9CiAgXSwKICAidG8iOiAic2VjcmV0MXBsMnIzMno5bjNlNDc5NTBzNmVqN21nN3BmaDdteGQwMDNndXVtIiwKICAiZXN0aW1hdGVkX2Ftb3VudCI6ICIyNTAiLAogICJtaW5pbXVtX2FjY2VwdGFibGVfYW1vdW50IjogIjI1MCIKfQ==" }}' --from a -y --keyring-backend test --gas 9000000 --gas-prices=3.0uscrt

# BUTT -> sSCRT -> BUTT -> sSCRT
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"send": { "recipient": "secret15rrl3qjafxzlzguu5x29xh29pam35uetwpfnna", "amount": "300", "msg": "ewogICJob3BzIjogWwogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MTh2ZDhmcHd4emNrOTNxbHdnaGFqNmFyaDRwN2M1bjg5Nzh2c3lnIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjRDRDdGNjRCOUFERTY1MjAwRTU5NTIxNjI2NTkzMkEwQzc2ODlDNDgwNEJFN0I0QTVGOENFQkVEMjUwQkY3RUEiCiAgICAgICAgfQogICAgICB9LAogICAgICAic21hcnRfY29udHJhY3QiOiB7CiAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MW15M2p2bDZ6czJuMjc2NDh6bmdxcnR3OHBkMjNucmtyaDBmN2F4IiwKICAgICAgICAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIgogICAgICB9CiAgICB9LAogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MXZjNXpmd3QwOGh1OWNjdGRna3FrNmo5eTA1dzZ0eTB4YThnMjZzIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjM1RjVEQjJCQzVDRDU2ODE1RDEwQzdBNTY3RDY4MjdCRUNDQjhFQUY0NUJDM0ZBMDE2OTMwQzRBODIwOUVBNjkiCiAgICAgICAgfQogICAgICB9LAogICAgICAic21hcnRfY29udHJhY3QiOiB7CiAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MW15M2p2bDZ6czJuMjc2NDh6bmdxcnR3OHBkMjNucmtyaDBmN2F4IiwKICAgICAgICAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIgogICAgICB9CiAgICB9LAogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MTh2ZDhmcHd4emNrOTNxbHdnaGFqNmFyaDRwN2M1bjg5Nzh2c3lnIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjRDRDdGNjRCOUFERTY1MjAwRTU5NTIxNjI2NTkzMkEwQzc2ODlDNDgwNEJFN0I0QTVGOENFQkVEMjUwQkY3RUEiCiAgICAgICAgfQogICAgICB9LAogICAgICAic21hcnRfY29udHJhY3QiOiB7CiAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MW15M2p2bDZ6czJuMjc2NDh6bmdxcnR3OHBkMjNucmtyaDBmN2F4IiwKICAgICAgICAiY29udHJhY3RfaGFzaCI6ICIyRDVBMDVFNzJGM0Y2RTZGRDZGNjRDRTY2MzdCODJBQUYyNDYwOEM0Q0Q2M0E2RUMxMEY3M0NCNjA4QzA5OEZDIgogICAgICB9CiAgICB9LAogICAgewogICAgICAiZnJvbV90b2tlbiI6IHsKICAgICAgICAic25pcDIwIjogewogICAgICAgICAgImFkZHJlc3MiOiAic2VjcmV0MXZjNXpmd3QwOGh1OWNjdGRna3FrNmo5eTA1dzZ0eTB4YThnMjZzIiwKICAgICAgICAgICJjb250cmFjdF9oYXNoIjogIjM1RjVEQjJCQzVDRDU2ODE1RDEwQzdBNTY3RDY4MjdCRUNDQjhFQUY0NUJDM0ZBMDE2OTMwQzRBODIwOUVBNjkiCiAgICAgICAgfQogICAgICB9CiAgICB9CiAgXSwKICAidG8iOiAic2VjcmV0MWprN3o2ZGhuOXRlM2poOWQ1c3hjbTh6dTA4N3prejN0cHh0bWZlIiwKICAiZXN0aW1hdGVkX2Ftb3VudCI6ICIyNTAiLAogICJtaW5pbXVtX2FjY2VwdGFibGVfYW1vdW50IjogIjI1MCIKfQ==" }}' --from b -y --keyring-backend test --gas 9000000 --gas-prices=3.0uscrt

# Query that sSCRT was sent to contract initiator
secretcli query compute query secret1vc5zfwt08hu9cctdgkqk6j9y05w6ty0xa8g26s '{"balance": {"address": "secret1pl2r32z9n3e47950s6ej7mg7pfh7mxd003guum", "key": "DoTheRightThing."}}'
```

## References
1. DEX aggregator: https://btn.group/secret_network/dex_aggregator
2. Secret contracts guide: https://github.com/enigmampc/secret-contracts-guide
3. 1inch BSC aggregator conract: https://bscscan.com/address/0x1111111254fb6c44bAC0beD2854e76F90643097d#code
4. AutoSwap aggregator contract: https://bscscan.com/address/0xbd6ed39bedf95517c45ddb3a35b647a462eed55d#code
