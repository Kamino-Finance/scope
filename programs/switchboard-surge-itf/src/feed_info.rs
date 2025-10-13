
//! Packed feed information data structures for oracle quotes
//!
//! This module defines zero-copy data structures for efficiently storing and accessing
//! oracle feed data within quotes. The structures use `#[repr(packed)]` to ensure
//! consistent memory layout across platforms and minimize space usage.

use rust_decimal::prelude::*;

use crate::prelude::*;

/// Packed quote header containing the signed slot hash
///
/// This header is signed by all oracles in the quote and contains the slot hash
/// that is used to validate the quote's freshness against the slot hash sysvar.
///
/// Size: 32 bytes
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(packed)]
pub struct PackedQuoteHeader {
    /// The 32-byte slot hash that was signed by all oracles in the quote
    pub signed_slothash: [u8; 32],
}

// Custom Borsh implementation for packed struct
impl borsh::BorshSerialize for PackedQuoteHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Read values without taking references (safe for packed structs)
        let slothash = self.signed_slothash;
        writer.write_all(&slothash)?;
        Ok(())
    }
}

impl borsh::BorshDeserialize for PackedQuoteHeader {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut signed_slothash = [0u8; 32];
        reader.read_exact(&mut signed_slothash)?;
        Ok(Self { signed_slothash })
    }
}

/// Packed feed information containing ID, value, and validation requirements
///
/// This structure stores individual feed data within a quote. The layout is optimized
/// for compatibility with JavaScript serialization, with the feed ID first, followed
/// by the value and minimum sample requirement.
///
/// Size: 49 bytes (32 + 16 + 1)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(packed)]
pub struct PackedFeedInfo {
    /// 32-byte unique identifier for this feed
    pub feed_id: [u8; 32],
    /// Feed value as a fixed-point integer (scaled by PRECISION)
    pub feed_value: i128,
    /// Minimum number of oracle samples required for this feed to be considered valid
    pub min_oracle_samples: u8,
}

// Custom Borsh implementation for packed struct
impl borsh::BorshSerialize for PackedFeedInfo {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Read values without taking references (safe for packed structs)
        let feed_id = self.feed_id;
        let feed_value = self.feed_value;
        let min_oracle_samples = self.min_oracle_samples;

        writer.write_all(&feed_id)?;
        writer.write_all(&feed_value.to_le_bytes())?;
        writer.write_all(&[min_oracle_samples])?;
        Ok(())
    }
}

impl borsh::BorshDeserialize for PackedFeedInfo {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut feed_id = [0u8; 32];
        reader.read_exact(&mut feed_id)?;

        let mut value_bytes = [0u8; 16];
        reader.read_exact(&mut value_bytes)?;
        let feed_value = i128::from_le_bytes(value_bytes);

        let mut min_samples = [0u8; 1];
        reader.read_exact(&mut min_samples)?;
        let min_oracle_samples = min_samples[0];

        Ok(Self {
            feed_id,
            feed_value,
            min_oracle_samples,
        })
    }
}

impl PackedFeedInfo {
    /// The size in bytes of this packed structure
    pub const PACKED_SIZE: usize = 49;

    /// Returns a reference to the 32-byte feed ID
    #[inline(always)]
    pub fn feed_id(&self) -> &[u8; 32] {
        &self.feed_id
    }

    /// Returns the raw feed value as a fixed-point integer
    ///
    /// This value is scaled by the program-wide `PRECISION` constant.
    /// Use [`value()`](Self::value) to get the human-readable decimal form.
    #[inline(always)]
    pub fn feed_value(&self) -> i128 {
        self.feed_value
    }

    /// Returns the feed value as a `Decimal`, scaled using the program-wide `PRECISION`.
    ///
    /// This converts the raw fixed-point integer into a human-readable decimal form.
    /// For example, if the raw value is 115525650000000000000000 and PRECISION is 18,
    /// this will return approximately 115525.65.
    #[inline(always)]
    pub fn value(&self) -> Decimal {
        Decimal::from_i128_with_scale(self.feed_value(), PRECISION).normalize()
    }

    /// Returns the minimum number of oracle samples required for this feed
    #[inline(always)]
    pub fn min_oracle_samples(&self) -> u8 {
        self.min_oracle_samples
    }
}

