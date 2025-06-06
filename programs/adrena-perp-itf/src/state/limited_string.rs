use {
    anchor_lang::prelude::*,
    bytemuck::{Pod, Zeroable},
    std::fmt::Display,
};

// Storage space must be known in advance, as such all strings are limited to 64 characters.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Zeroable, Pod)]
#[repr(C)]
pub struct LimitedString {
    pub value: [u8; 31],
    pub length: u8,
}

impl std::fmt::Debug for LimitedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl PartialEq for LimitedString {
    fn eq(&self, other: &Self) -> bool {
        self.length == other.length
            && self.value[..self.length as usize] == other.value[..other.length as usize]
    }
}

impl From<LimitedString> for String {
    fn from(limited_string: LimitedString) -> Self {
        let mut string = String::new();
        for byte in limited_string.value.iter() {
            if *byte == 0 {
                break;
            }
            string.push(*byte as char);
        }
        string
    }
}

impl LimitedString {
    pub fn to_bytes(&self) -> &[u8] {
        &self.value[..self.length as usize]
    }
}

impl Display for LimitedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(*self))
    }
}

impl Default for LimitedString {
    fn default() -> Self {
        Self {
            value: [0; Self::MAX_LENGTH],
            length: 0,
        }
    }
}

impl LimitedString {
    pub const MAX_LENGTH: usize = 31;

    pub fn new<S: AsRef<str>>(input: S) -> Self {
        let input_str = input.as_ref();
        let length = input_str.len() as u8;
        let mut array = [0; Self::MAX_LENGTH];
        let bytes = input_str.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if index >= Self::MAX_LENGTH {
                break;
            }
            array[index] = *byte;
        }
        LimitedString {
            value: array,
            length,
        }
    }

    pub const fn new_const(input: &'static str) -> Self {
        let length = input.len() as u8;
        let bytes = input.as_bytes();
        let mut array = [0; Self::MAX_LENGTH];
        let mut i = 0;

        while i < Self::MAX_LENGTH && i < length as usize {
            array[i] = bytes[i];
            i += 1;
        }

        LimitedString {
            value: array,
            length,
        }
    }

    // Converts the LimitedString into a [u8; 32], zero-padded. Useful in tests.
    pub fn to_fixed_32(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];

        let len = self.length as usize;

        let copy_len = len.min(31);

        buf[..copy_len].copy_from_slice(&self.value[..copy_len]);
        buf[31] = self.length; // mirror the struct layout: 31 bytes + 1 byte len
        buf
    }
}
