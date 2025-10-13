use crate::borsh;

/// ED25519 signature data offsets within instruction data
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Ed25519SignatureOffsets {
    /// Offset to the signature data
    pub signature_offset: u16,
    /// Instruction index containing the signature
    pub signature_instruction_index: u16,
    /// Offset to the public key data
    pub public_key_offset: u16,
    /// Instruction index containing the public key
    pub public_key_instruction_index: u16,
    /// Offset to the message data
    pub message_data_offset: u16,
    /// Size of the message data in bytes
    pub message_data_size: u16,
    /// Instruction index containing the message
    pub message_instruction_index: u16,
}
