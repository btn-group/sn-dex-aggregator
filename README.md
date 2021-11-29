# Password manager

## Testing locally examples
```
# Run chain locally
docker run -it --rm -p 26657:26657 -p 26656:26656 -p 1337:1337 -v $(pwd):/root/code --name secretdev enigmampc/secret-network-sw-dev

# Access container via separate terminal window 
docker exec -it secretdev /bin/bash

# cd into code folder
cd code

# Store contracts required for test
secretcli tx compute store buttcoin.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store password-manager.wasm.gz --from a --gas 3000000 -y --keyring-backend test
secretcli tx compute store butt-lode.wasm.gz --from a --gas 3000000 -y --keyring-backend test

# Get the contract's id
secretcli query compute list-code

# Init Buttcoin 
CODE_ID=1
INIT='{"name": "Buttcoin", "symbol": "BUTT", "decimals": 6, "initial_balances": [{"address": "secret1j57c8crjnkdmfufha044cra8qynrhhndtehnz5", "amount": "1000000000000000000"},{"address": "secret1vglh0uqsce9hayzxp5p96nakppvmqjc43ewp53", "amount": "1000000000000000000"}], "prng_seed": "testing"}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "Buttcoin" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Set viewing key for Buttcoin
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"set_viewing_key": { "key": "testing" }}' --from a -y --keyring-backend test
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"set_viewing_key": { "key": "testing" }}' --from b -y --keyring-backend test

# Init BUTT lode
CODE_ID=3
INIT='{"viewing_key": "DoTheRightThing.", "time_delay": 5}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "butt-lode" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Init password manager
CODE_ID=5
INIT='{"buttcoin": {"address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg", "contract_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA"}, "butt_lode": {"address": "secret174kgn5rtw4kf6f938wm7kwh70h2v4vcfft5mqy", "contract_hash": "99F94EDC0D744B35A8FBCBDC8FB71C140CFA8F3F91FAD8C35B7CC37862A4AC95"}}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "password manager - btn.group" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Query config for address alias
secretcli query compute query $CONTRACT_INSTANCE_ADDRESS '{"config": {}}'

# Query by address
secretcli query compute query secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek '{"search": {"search_type": "address", "search_value": "secret1zm55tcme6epjl4jt30v05gh9xetyp9e3vvv6nr"}}'

# Query by alias
secretcli query compute query secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek '{"search": {"search_type": "alias", "search_value": "btn.group admin"}}'

# Create alias
secretcli tx compute execute secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"send": { "recipient": "secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek", "amount": "1000000", "msg": "eyJjcmVhdGUiOnsiYWxpYXMiOiAiYWRmYXNkZmEiLCJhdmF0YXJfdXJsIjogImh0dHBzOi8vc2VjcmV0bm9kZXMuY29tL2Fzc2V0cy9odWJibGUtbG9nby03M2JkN2FjYzI2YmYxNmM0YWY5NjZiZWE2Yjk0ZTY4MDliMTBkNzNmOTllMTJiNTU4YTc4OGQ2OTdiYjdjY2Q0LnBuZyJ9fQ" }}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Query by address
secretcli query compute query secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek '{"search": {"search_type": "address", "search_value": "secret1j57c8crjnkdmfufha044cra8qynrhhndtehnz5"}}'

# Destroy alias
secretcli tx compute execute secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek '{"destroy": {"alias": "adfasdfa"}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt

# Query that BUTT was sent to BUTT lode
secretcli tx compute execute secret1tndcaqxkpc5ce9qee5ggqf430mr2z3pedc68dx '{"set_viewing_key_for_snip20": {"token": {"address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg", "contract_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA"}}}' --from a -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt
secretcli query compute query secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg '{"balance": {"address": "secret1tndcaqxkpc5ce9qee5ggqf430mr2z3pedc68dx", "key": "DoTheRightThing."}}'
```

## References
1. Address alias: https://btn.group/secret_network/password_manager
2. Secret contracts guide: https://github.com/enigmampc/secret-contracts-guide
