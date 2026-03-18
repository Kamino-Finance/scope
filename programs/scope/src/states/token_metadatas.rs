use anchor_lang::prelude::*;

use crate::MAX_ENTRIES;

#[account(zero_copy)]
pub struct TokenMetadatas {
    pub metadatas_array: [TokenMetadata; MAX_ENTRIES],
}

#[zero_copy]
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Default)]
pub struct TokenMetadata {
    pub name: [u8; 32],
    pub max_age_price_slots: u64,
    pub group_ids_bitset: u64, // a bitset of group IDs in range [0, 64).
    pub _reserved: [u64; 15],
}

impl TokenMetadata {
    pub fn get_name(&self) -> &str {
        std::str::from_utf8(&self.name)
            .unwrap()
            .trim_end_matches('\0')
    }

    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let mut padded_name = [0_u8; 32];
        padded_name[..bytes.len()].copy_from_slice(bytes);
        self.name = padded_name;
    }

    pub fn reset(&mut self) {
        *self = TokenMetadata::default();
    }
}

impl std::fmt::Debug for TokenMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenMetadata")
            .field("name", &self.get_name())
            .field("max_age_price_slots", &self.max_age_price_slots)
            .field(
                "group_ids_bitset",
                &list_set_bit_positions(self.group_ids_bitset),
            )
            .finish()
    }
}

// Function placed here to allow usage in both scope and scope-types.
/// Lists the bit positions (where LSB == 0) of all the set bits (i.e. `1`s) in the given number's
/// binary representation.
pub fn list_set_bit_positions(mut bits: u64) -> Vec<u8> {
    let mut positions = Vec::with_capacity(usize::try_from(bits.count_ones()).unwrap());
    while bits != 0 {
        let position = u8::try_from(bits.trailing_zeros()).unwrap();
        positions.push(position);
        bits &= bits.wrapping_sub(1);
    }
    positions
}
