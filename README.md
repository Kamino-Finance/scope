# Scope

_Scope sees all prices in one glance._

Scope is a price oracle aggregator living on the Solana network. It copies data from multiple on-chain oracles' accounts into one "price feed".

Scope pre-validate the prices with a preset of rules and perform the update only if they meet the criteria.

The repository contains two useful codebases:

- [`scope`](./programs/scope/) on-chain program.
- [`scope-types`](./programs/scope/types) to use as a dependency when accessing Scope prices in another smart contract.

## Limitations

- The association between a price at a given index in the price feed and the token pair associated with this price need is not stored on-chain. The label might indicate this association.
- A price feed is currently limited to 512 prices.
- If you do not have access to the Kamino source code, Scope can still be built. See [Building without Kamino ktokens](#building-without-kamino-ktokens) for more details.

### Building without Kamino kTokens

If you do not have access to the Kamino source code, you can still build scope without the default `yvaults` feature:

- Replace the `yvaults` dependency in `./programs/scope/Cargo.toml` with the `yvaults_stub` package:

```toml
[dependencies]
# Comment out the git repo
#yvaults = { git = "ssh://git@github.com/hubbleprotocol/yvault.git", features = ["no-entrypoint", "cpi", "mainnet"], optional = true }

# Add this line
yvaults = { path = "../yvaults_stub", package = "yvaults_stub", optional = true }
```

- Build scope with the following command:

```sh
anchor build -p scope -- --no-default-features --features mainnet
```
