use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;

use crate::{
    utils::{consts::FULL_BPS, math, zero_copy_deserialize},
    warn, DatedPrice, Price,
};

/// Jito restaking price oracle gives the amount of JitoSOL per VRT token on withdrawal
/// WARNING: Assumes both tokens have the same decimals (9)
pub fn get_price(jito_vault: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let vault = zero_copy_deserialize::<jito_vault_core::Vault>(jito_vault)?;

    let dated_price = DatedPrice {
        price: get_price_int(&vault),
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp).unwrap(),
        ..Default::default()
    };

    Ok(dated_price)
}

fn get_price_int(vault: &jito_vault_core::Vault) -> Price {
    let vrt_supply = vault.vrt_supply.get();
    if vrt_supply == 0 {
        return Price::default();
    }

    let total_deposits = vault.tokens_deposited.get();

    let total_fee_bps = vault.program_fee_bps.get() + vault.withdrawal_fee_bps.get();

    let withdrawable_amount = math::mul_bps(total_deposits, FULL_BPS.saturating_sub(total_fee_bps));

    let price_dec = Decimal::from(withdrawable_amount) / vrt_supply;
    price_dec.into()
}

pub fn validate_account(vault: &Option<AccountInfo>) -> Result<()> {
    let Some(vault) = vault else {
        warn!("No vault account provided");
        return err!(crate::ScopeError::UnexpectedAccount);
    };
    let _ = zero_copy_deserialize::<jito_vault_core::Vault>(vault)?;
    Ok(())
}

pub mod jito_vault_core {
    use anchor_lang::Discriminator;
    use bytemuck::{Pod, Zeroable};

    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
    #[repr(C)]
    pub struct DelegationState {
        /// The amount of stake that is currently active on the operator
        staked_amount: PodU64,

        /// Any stake that was deactivated in the current epoch
        enqueued_for_cooldown_amount: PodU64,

        /// Any stake that was deactivated in the previous epoch,
        /// to be available for re-delegation in the current epoch + 1
        cooling_down_amount: PodU64,

        reserved: [u8; 256],
    }

    #[derive(Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
    #[repr(C)]
    pub struct Vault {
        /// The base account of the VRT
        pub base: Pubkey,

        // ------------------------------------------
        // Token information and accounting
        // ------------------------------------------
        /// Mint of the VRT token
        pub vrt_mint: Pubkey,

        /// Mint of the token that is supported by the VRT
        pub supported_mint: Pubkey,

        /// The total number of VRT in circulation
        pub vrt_supply: PodU64,

        /// The total number of tokens deposited
        pub tokens_deposited: PodU64,

        /// The maximum deposit capacity allowed in the mint_to instruction.
        /// The deposited assets in the vault may exceed the deposit_capacity during other operations, such as vault balance updates.
        pub deposit_capacity: PodU64,

        /// Rolled-up stake state for all operators in the set
        pub delegation_state: DelegationState,

        /// The amount of additional assets that need unstaking to fulfill VRT withdrawals
        pub additional_assets_need_unstaking: PodU64,

        /// The amount of VRT tokens in VaultStakerWithdrawalTickets enqueued for cooldown
        pub vrt_enqueued_for_cooldown_amount: PodU64,

        /// The amount of VRT tokens cooling down
        pub vrt_cooling_down_amount: PodU64,

        /// The amount of VRT tokens ready to claim
        pub vrt_ready_to_claim_amount: PodU64,

        // ------------------------------------------
        // Admins
        // ------------------------------------------
        /// Vault admin
        pub admin: Pubkey,

        /// The delegation admin responsible for adding and removing delegations from operators.
        pub delegation_admin: Pubkey,

        /// The operator admin responsible for adding and removing operators.
        pub operator_admin: Pubkey,

        /// The node consensus network admin responsible for adding and removing support for NCNs.
        pub ncn_admin: Pubkey,

        /// The admin responsible for adding and removing slashers.
        pub slasher_admin: Pubkey,

        /// The admin responsible for setting the capacity
        pub capacity_admin: Pubkey,

        /// The admin responsible for setting the fees
        pub fee_admin: Pubkey,

        /// The delegate_admin responsible for delegating assets
        pub delegate_asset_admin: Pubkey,

        /// Fee wallet account
        pub fee_wallet: Pubkey,

        /// Optional mint signer
        pub mint_burn_admin: Pubkey,

        /// ( For future use ) Authority to update the vault's metadata
        pub metadata_admin: Pubkey,

        // ------------------------------------------
        // Indexing and counters
        // These are helpful when one needs to iterate through all the accounts
        // ------------------------------------------
        /// The index of the vault in the vault list
        pub vault_index: PodU64,

        /// Number of VaultNcnTicket accounts tracked by this vault
        pub ncn_count: PodU64,

        /// Number of VaultOperatorDelegation accounts tracked by this vault
        pub operator_count: PodU64,

        /// Number of VaultNcnSlasherTicket accounts tracked by this vault
        pub slasher_count: PodU64,

        /// The slot of the last fee change
        pub last_fee_change_slot: PodU64,

        /// The slot of the last time the delegations were updated
        pub last_full_state_update_slot: PodU64,

        /// The deposit fee in basis points
        pub deposit_fee_bps: PodU16,

        /// The withdrawal fee in basis points
        pub withdrawal_fee_bps: PodU16,

        /// The next epoch's withdrawal fee in basis points
        pub next_withdrawal_fee_bps: PodU16,

        /// Fee for each epoch
        pub reward_fee_bps: PodU16,

        /// (Copied from Config) The program fee in basis points
        pub program_fee_bps: PodU16,

        /// The bump seed for the PDA
        pub bump: u8,

        pub is_paused: u8,

        /// Reserved space
        pub reserved: [u8; 259],
    }

    impl Discriminator for Vault {
        const DISCRIMINATOR: [u8; 8] = [2, 0, 0, 0, 0, 0, 0, 0];
        fn discriminator() -> [u8; 8] {
            Self::DISCRIMINATOR
        }
    }

    impl Default for Vault {
        fn default() -> Self {
            Zeroable::zeroed()
        }
    }

    #[derive(Clone, Copy, Default, PartialEq, Pod, Zeroable, Eq)]
    #[repr(transparent)]
    pub struct PodU64([u8; 8]);

    impl PodU64 {
        pub fn get(&self) -> u64 {
            u64::from_le_bytes(self.0)
        }

        pub fn set(&mut self, value: u64) {
            self.0 = value.to_le_bytes();
        }
    }

    impl From<u64> for PodU64 {
        fn from(value: u64) -> Self {
            PodU64(value.to_le_bytes())
        }
    }

    #[derive(Clone, Copy, Default, PartialEq, Pod, Zeroable, Eq)]
    #[repr(transparent)]
    pub struct PodU16([u8; 2]);

    impl PodU16 {
        pub fn get(&self) -> u16 {
            u16::from_le_bytes(self.0)
        }

        pub fn set(&mut self, value: u16) {
            self.0 = value.to_le_bytes();
        }
    }

    impl From<u16> for PodU16 {
        fn from(value: u16) -> Self {
            PodU16(value.to_le_bytes())
        }
    }
}
