use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    states::OraclePrices, utils::consts::FULL_BPS, warn, DatedPrice, Price, ScopeError,
    ScopeResult, MAX_ENTRIES_U16,
};

/// Maximum number of source entries for Conditional (A, B, and optionally C for WithinRangeAbs/OutsideRangeAbs).
pub const CONDITIONAL_MAX_SOURCES: usize = 3;

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    AnchorSerialize,
    AnchorDeserialize,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Condition {
    /// A > B
    #[default]
    Gt = 0,
    /// A >= B
    Gte = 1,
    /// A < B
    Lt = 2,
    /// A <= B
    Lte = 3,
    /// A == B
    Eq = 4,
    /// A != B
    Neq = 5,
    /// A is within [min(B, C), max(B, C)]
    WithinRangeAbs = 6,
    /// A is outside [min(B, C), max(B, C)]
    OutsideRangeAbs = 7,
    /// A is within `tolerance_bps` of reference B:
    /// |A - B| <= B * tolerance_bps / 10000.
    /// This condition is intentionally asymmetric; swapping A and B can change the result.
    /// Edge case: when B == 0 the right-hand side is 0, so the condition reduces to |A| <= 0 and is
    /// true only when A == 0 (regardless of `tolerance_bps`).
    WithinRangeBps = 8,
    /// A is outside `tolerance_bps` of reference B:
    /// |A - B| > B * tolerance_bps / 10000.
    /// This condition is intentionally asymmetric; swapping A and B can change the result.
    /// Edge case: when B == 0 the right-hand side is 0, so the condition reduces to |A| > 0 and is
    /// true whenever A != 0 (regardless of `tolerance_bps`).
    OutsideRangeBps = 9,
    /// A > 0 (only uses source A)
    NonZero = 10,
}

#[derive(Debug, Default, AnchorDeserialize, AnchorSerialize)]
pub struct ConditionalData {
    pub condition: u8,
    pub tolerance_bps: u16,
    /// Extension-prone source list is stored last so future versioned layouts can
    /// add more sources without shifting earlier scalar fields.
    pub sources: [u16; CONDITIONAL_MAX_SOURCES],
}

impl ConditionalData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize ConditionalData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize ConditionalData");
        buff
    }

    fn condition(&self) -> ScopeResult<Condition> {
        Condition::try_from(self.condition).map_err(|_| {
            msg!("Invalid Condition discriminant: {}", self.condition);
            ScopeError::InvalidGenericData
        })
    }
}

fn get_source_price(oracle_prices: &OraclePrices, index: u16) -> ScopeResult<&DatedPrice> {
    if index >= MAX_ENTRIES_U16 {
        return Err(ScopeError::CompositeOracleInvalidSourceIndex);
    }
    oracle_prices
        .prices
        .get(usize::from(index))
        .ok_or(ScopeError::CompositeOracleInvalidSourceIndex)
}

fn bool_to_dated_price(result: bool, sources: &[&DatedPrice]) -> DatedPrice {
    // `sources` is always non-empty by construction (callers pass hard-coded
    // arrays of 1–3 entries); `.unwrap()` surfaces a future regression that
    // accidentally introduces an empty-call path
    let oldest_timestamp = sources.iter().map(|p| p.unix_timestamp).min().unwrap();
    let oldest_slot = sources.iter().map(|p| p.last_updated_slot).min().unwrap();

    DatedPrice {
        price: Price {
            value: u64::from(result),
            exp: 0,
        },
        unix_timestamp: oldest_timestamp,
        last_updated_slot: oldest_slot,
        ..Default::default()
    }
}

pub fn get_price(oracle_prices: &OraclePrices, generic_data: &[u8]) -> ScopeResult<DatedPrice> {
    let data = ConditionalData::from_generic_data(generic_data)?;
    let condition = data.condition()?;

    let price_a = get_source_price(oracle_prices, data.sources[0])?;
    let dec_a = Decimal::from(price_a.price);

    match condition {
        Condition::NonZero => Ok(bool_to_dated_price(price_a.price.value > 0, &[price_a])),

        Condition::Gt
        | Condition::Gte
        | Condition::Lt
        | Condition::Lte
        | Condition::Eq
        | Condition::Neq
        | Condition::WithinRangeBps
        | Condition::OutsideRangeBps => {
            let price_b = get_source_price(oracle_prices, data.sources[1])?;
            let dec_b = Decimal::from(price_b.price);
            let result = match condition {
                Condition::Gt => dec_a > dec_b,
                Condition::Gte => dec_a >= dec_b,
                Condition::Lt => dec_a < dec_b,
                Condition::Lte => dec_a <= dec_b,
                Condition::Eq => dec_a == dec_b,
                Condition::Neq => dec_a != dec_b,
                Condition::WithinRangeBps | Condition::OutsideRangeBps => {
                    let diff = if dec_a > dec_b {
                        dec_a - dec_b
                    } else {
                        dec_b - dec_a
                    };
                    // Tolerance is a fraction of the reference B, so when B == 0 it is 0 and the
                    // check reduces to an exact comparison of A against 0 (see the WithinRangeBps /
                    // OutsideRangeBps docs).
                    let tolerance = dec_b * u64::from(data.tolerance_bps) / u64::from(FULL_BPS);
                    let within = diff <= tolerance;
                    if condition == Condition::WithinRangeBps {
                        within
                    } else {
                        !within
                    }
                }
                Condition::NonZero | Condition::WithinRangeAbs | Condition::OutsideRangeAbs => {
                    unreachable!("excluded by outer match arm")
                }
            };
            Ok(bool_to_dated_price(result, &[price_a, price_b]))
        }

        Condition::WithinRangeAbs | Condition::OutsideRangeAbs => {
            let price_b = get_source_price(oracle_prices, data.sources[1])?;
            let price_c = get_source_price(oracle_prices, data.sources[2])?;
            let dec_b = Decimal::from(price_b.price);
            let dec_c = Decimal::from(price_c.price);
            let (lo, hi) = if dec_b <= dec_c {
                (dec_b, dec_c)
            } else {
                (dec_c, dec_b)
            };
            let within = lo <= dec_a && dec_a <= hi;
            let result = if condition == Condition::WithinRangeAbs {
                within
            } else {
                !within
            };
            Ok(bool_to_dated_price(result, &[price_a, price_b, price_c]))
        }
    }
}

pub fn validate_mapping_cfg(
    mapping: Option<&AccountInfo>,
    generic_data: &[u8],
    own_index: u16,
) -> ScopeResult<()> {
    if mapping.is_some() {
        warn!("No mapping account is expected for Conditional oracle");
        return Err(ScopeError::PriceAccountNotExpected);
    }

    let data = ConditionalData::from_generic_data(generic_data)?;
    let condition = data.condition()?;

    // Source A is required by every condition.
    require_valid_source(data.sources[0], "A")?;
    reject_self_source(data.sources[0], own_index, "A")?;

    match condition {
        Condition::NonZero => {
            // Only source A is needed.
        }
        Condition::Gt
        | Condition::Gte
        | Condition::Lt
        | Condition::Lte
        | Condition::Eq
        | Condition::Neq => {
            require_valid_source(data.sources[1], "B")?;
            reject_self_source(data.sources[1], own_index, "B")?;
            warn_if_same_source(data.sources[0], "A", data.sources[1], "B");
        }
        Condition::WithinRangeAbs | Condition::OutsideRangeAbs => {
            require_valid_source(data.sources[1], "B")?;
            require_valid_source(data.sources[2], "C")?;
            reject_self_source(data.sources[1], own_index, "B")?;
            reject_self_source(data.sources[2], own_index, "C")?;
            warn_if_same_source(data.sources[0], "A", data.sources[1], "B");
            warn_if_same_source(data.sources[0], "A", data.sources[2], "C");
            // B == C collapses the range [min(B,C), max(B,C)] to a single point,
            // silently turning WithinRangeAbs into Eq(A, B) and OutsideRangeAbs
            // into Neq(A, B); flag this so the operator notices.
            warn_if_same_source(data.sources[1], "B", data.sources[2], "C");
        }
        Condition::WithinRangeBps | Condition::OutsideRangeBps => {
            require_valid_source(data.sources[1], "B")?;
            reject_self_source(data.sources[1], own_index, "B")?;
            if data.tolerance_bps == 0 || data.tolerance_bps > FULL_BPS {
                warn!(
                    "WithinRangeBps/OutsideRangeBps condition requires 0 < tolerance_bps <= FULL_BPS ({}); got {}",
                    FULL_BPS, data.tolerance_bps
                );
                return Err(ScopeError::InvalidGenericData);
            }
            warn_if_same_source(data.sources[0], "A", data.sources[1], "B");
        }
    }

    msg!(
        "Validate Conditional: condition={:?}, sources={:?}, tolerance_bps={}",
        condition,
        data.sources,
        data.tolerance_bps
    );

    Ok(())
}

fn require_valid_source(idx: u16, name: &str) -> ScopeResult<()> {
    if idx >= MAX_ENTRIES_U16 {
        warn!("Conditional requires a valid source {} entry", name);
        return Err(ScopeError::OracleConfigInvalidSourceIndices);
    }
    Ok(())
}

fn reject_self_source(idx: u16, own_index: u16, name: &str) -> ScopeResult<()> {
    if idx == own_index {
        warn!(
            "Conditional source {} (index {}) is the entry's own index; self-reference is not allowed",
            name, idx
        );
        return Err(ScopeError::OracleConfigInvalidSourceIndices);
    }
    Ok(())
}

fn warn_if_same_source(idx1: u16, name1: &str, idx2: u16, name2: &str) {
    if idx1 == idx2 {
        warn!(
            "Conditional: source {} and source {} are the same (index {}), likely a misconfiguration",
            name1, name2, idx1
        );
    }
}
