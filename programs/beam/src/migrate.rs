//! Deprecated compatibility interface. No foreign program accounts or CPIs.
use crate::*;
#[derive(Accounts)]
pub struct Migrate<'info> {
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub global: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub withdraw_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub mint: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub bonding_curve: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_bonding_curve: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub system_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_authority_mint_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_authority_wsol_account: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub amm_global_config: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub wsol_mint: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub lp_mint: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub user_pool_token_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_base_token_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_quote_token_account: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_2022_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm_event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub rent: UncheckedAccount<'info>,
}

pub fn execute<'info>(_ctx: Context<'_, '_, '_, 'info, Migrate<'info>>) -> Result<()> {
    crate::beam_policy::legacy_amm()
}

#[event]
#[derive(Clone)]
pub struct CompletePumpAmmMigrationEvent {
    pub user: Pubkey,
    pub mint: Pubkey,
    pub mint_amount: u64,
    pub sol_amount: u64,
    pub pool_migration_fee: u64,
    pub bonding_curve: Pubkey,
    pub timestamp: i64,
    pub pool: Pubkey,
    pub quote_mint: Pubkey,
}

pub use crate::payments::migrate_transfer_native as transfer_native;
#[derive(Clone)]
pub struct DistributionCurve {
    pub creator: Pubkey,
    pub quote_mint: Pubkey,
}
impl Owner for DistributionCurve {
    fn owner() -> Pubkey {
        crate::ID
    }
}
impl AccountSerialize for DistributionCurve {}
impl AccountDeserialize for DistributionCurve {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self> {
        Self::try_deserialize_unchecked(buf)
    }
    fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self> {
        if buf.len() < 49 || buf[..8] != [23, 183, 248, 55, 96, 216, 172, 96] {
            return Err(
                anchor_lang::solana_program::program_error::ProgramError::InvalidAccountData.into(),
            );
        }
        for offset in [48usize, 81, 82, 123, 124] {
            if buf.len() > offset {
                let _ = bool::deserialize(&mut &buf[offset..offset + 1])?;
            }
        }
        let creator = if buf.len() >= 81 {
            Pubkey::new_from_array(buf[49..81].try_into().unwrap_or_default())
        } else {
            Pubkey::default()
        };
        let quote_mint = if buf.len() >= 115 {
            Pubkey::new_from_array(buf[83..115].try_into().unwrap_or_default())
        } else {
            Pubkey::default()
        };
        Ok(Self {
            creator,
            quote_mint,
        })
    }
}
