//! Deprecated compatibility interface. No foreign program accounts or CPIs.
use crate::*;
#[derive(Accounts)]
pub struct AdminCto<'info> {
    #[account(mut)]
    pub admin_set_creator_authority: Signer<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub global: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub quote_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub system_program: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub bonding_curve: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub current_creator: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub current_creator_quote_token_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub creator_vault: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub creator_vault_quote_token_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub holder_creator_vault: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub holder_creator_vault_quote_token_account: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub amm_global_config: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pool: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm_event_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub coin_creator_vault_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub coin_creator_vault_ata: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub sharing_config: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_fees: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_fees_event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub program: UncheckedAccount<'info>,
}

pub fn execute(
    _ctx: Context<AdminCto>,
    _holder: Option<bool>,
    _fee: Option<u64>,
    _new: Option<Pubkey>,
) -> Result<()> {
    crate::beam_policy::legacy_amm()
}

#[event]
#[derive(Clone)]
pub struct AdminCtoEvent {
    pub timestamp: i64,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub old_creator: Pubkey,
    pub new_creator: Pubkey,
    pub is_holder_reward: bool,
    pub is_cashback_coin: bool,
    pub old_creator_fee_bps: u64,
    pub new_creator_fee_bps: u64,
    pub sharing_config_reset: bool,
    pub swept_to_holder_vault: u64,
    pub pool_updated: bool,
}
