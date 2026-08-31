//! Compatibility parsers for external wire formats that the pinned dependency versions cannot
//! provide. Each module owns only the decoding of one external format from raw bytes — account
//! context (owner checks) and business logic stay with the consumer.
//!
//! These modules are internal implementation details of scope's oracles (public only so the
//! integration tests and off-chain crates can reuse them).

pub mod token_2022_scaled_ui_amount;
