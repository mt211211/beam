//! Disabled Mayhem compatibility interface; no foreign authority or reserve update.
use crate::*;
#[derive(Accounts)]
pub struct SetMayhemVirtualParams<'info> {
    #[account(mut)]
    pub sol_vault_authority: Signer<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub mayhem_token_vault: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub global: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub bonding_curve: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Retained event-CPI ABI slot; this instruction always rejects.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained event-CPI ABI slot; this instruction always rejects.
    pub program: UncheckedAccount<'info>,
}

pub(crate) fn execute(_ctx: Context<SetMayhemVirtualParams>) -> Result<()> {
    crate::beam_policy::mayhem(true)
}

#[event]
#[derive(Clone, Copy)]
pub struct UpdateMayhemVirtualParamsEvent {
    pub timestamp: i64,
    pub mint: Pubkey,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub new_virtual_token_reserves: u64,
    pub new_virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
}
