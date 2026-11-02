//! Deprecated compatibility interface. No foreign program accounts or CPIs.
use crate::*;
#[derive(Accounts)]
pub struct MigrateV2<'info> {
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub global: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub withdraw_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub base_mint: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub quote_mint: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub bonding_curve: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_base_bonding_curve: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_quote_bonding_curve: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub system_program: UncheckedAccount<'info>,
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
    pub pool_authority_quote_account: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub amm_global_config: UncheckedAccount<'info>,
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
    pub base_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub quote_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_2022_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub associated_token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm_event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub rent: UncheckedAccount<'info>,
    /// CHECK: Retained event-CPI ABI slot; this instruction always rejects.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained event-CPI ABI slot; this instruction always rejects.
    pub program: UncheckedAccount<'info>,
}

pub fn execute<'info>(_ctx: Context<'_, '_, '_, 'info, MigrateV2<'info>>) -> Result<()> {
    crate::beam_policy::legacy_amm()
}

pub(crate) fn boost_enabled(_config: &AccountInfo) -> bool {
    false
}
