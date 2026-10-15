# Scope

_Scope sees all prices in one glance._

Scope is a price oracle aggregator living on the Solana network. It copies data from multiple on-chain oracles' accounts into one "price feed".

Scope pre-validate the prices with a preset of rules and perform the update only if they meet the criteria.

The repository contains two useful codebases:

- [`scope`](./programs/scope/) on-chain program.
- [`scope-types`](./programs/scope-types/) to use as a dependency when accessing Scope prices in another smart contract.

## Limitations

- The association between a price at a given index in the price feed and the token pair associated with this price need is not stored on-chain. The label might indicate this association.
- A price feed is currently limited to 512 prices.
