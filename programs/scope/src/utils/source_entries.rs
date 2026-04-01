use crate::{ScopeError, ScopeResult, MAX_ENTRIES_U16};

/// Validates a source entries array:
/// - At least one valid entry (< MAX_ENTRIES_U16) is required.
/// - Repeated zeros are rejected (catches zeroed/uninitialized generic_data).
/// - Valid entries must be contiguous at the start; sentinel values (>= MAX_ENTRIES_U16) only at the end.
pub fn validate_source_entries(entries: &[u16]) -> ScopeResult<()> {
    // At least one valid entry is required
    if entries.first().map_or(true, |&e| e >= MAX_ENTRIES_U16) {
        return Err(ScopeError::OracleConfigInvalidSourceIndices);
    }

    // Reject repeated zeros (e.g., zeroed/uninitialized generic_data)
    if entries.iter().filter(|&&e| e == 0).count() > 1 {
        return Err(ScopeError::OracleConfigInvalidSourceIndices);
    }

    // Valid entries must be contiguous at the start, sentinels only at the end
    entries
        .iter()
        .skip_while(|&&e| e < MAX_ENTRIES_U16) // skip valid entries at start
        .find(|&&e| e < MAX_ENTRIES_U16) // look for valid entry after sentinel
        .map_or(Ok(()), |_| {
            Err(ScopeError::OracleConfigInvalidSourceIndices)
        })
}
