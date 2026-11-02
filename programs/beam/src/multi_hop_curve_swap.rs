//! Disabled historical router; account slots retained without foreign authority.
use crate::*;
#[derive(Accounts)]
pub struct MultiHopCurve<'info> {
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub global: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub fee_config: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub pump_amm_global_config: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub source_authority: Signer<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub source: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub destination: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub buyback_fee_recipient: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub token_2022_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub system_program: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Retained ABI slot; the disabled handler never reads or writes it.
    pub program: UncheckedAccount<'info>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CurveRunResult {
    pub amount_out: u64,
    pub buyback_fee: u64,
}

pub fn execute<'info>(
    _ctx: Context<'_, '_, 'info, 'info, MultiHopCurve<'info>>,
    _amount_in: u64,
    _hop_fees: Vec<u8>,
) -> Result<CurveRunResult> {
    crate::beam_policy::legacy_amm()?;
    unreachable!("legacy_amm always rejects")
}

pub use crate::payments::trade_event;
