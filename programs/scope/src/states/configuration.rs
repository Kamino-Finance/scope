use anchor_lang::prelude::*;

// Configuration account of the program
#[account(zero_copy)]
pub struct Configuration {
    pub admin: Pubkey,
    pub oracle_mappings: Pubkey,
    pub oracle_prices: Pubkey,
    pub tokens_metadata: Pubkey,
    pub oracle_twaps: Pubkey,
    pub admin_cached: Pubkey,
    pub emergency_council: Pubkey,
    pub resume_authority: Pubkey,
    _padding: [u64; 1247],
}

impl Configuration {
    /// Admin can always resume; the `resume_authority` can resume only once it is set.
    pub fn can_resume(&self, key: &Pubkey) -> bool {
        *key == self.admin || is_active_delegate(&self.resume_authority, key)
    }

    /// Admin can freeze and unfreeze; the `emergency_council` can only freeze.
    pub fn can_freeze(&self, key: &Pubkey, freeze: bool) -> bool {
        *key == self.admin || (freeze && is_active_delegate(&self.emergency_council, key))
    }
}

/// A delegate authority counts as active only once it has been set to a
/// non-default pubkey; an unset (default) delegate grants no rights.
fn is_active_delegate(delegate: &Pubkey, key: &Pubkey) -> bool {
    *delegate != Pubkey::default() && delegate == key
}
