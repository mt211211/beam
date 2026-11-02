//! Immutable creator-selected native SOL sharing plans. Creator deposits
//! flow through a fixed sharing PDA; rounding dust remains until it is earned
//! by a beneficiary under cumulative floor accounting. No Pump Fees CPI.
use crate::beam_amm_v4_companion::{AmmV4Rights, RIGHTS_SEED};
use anchor_lang::prelude::*;

pub const SHARING_SEED: &[u8] = b"beam-creator-sharing";
pub const MAX_RECIPIENTS: usize = 8;

#[account]
pub struct AmmV4Sharing {
    pub pool: Pubkey,
    pub creator: Pubkey,
    pub creation_revision: u64,
    pub distributed: u128,
    pub recipients: Vec<Pubkey>,
    pub basis_points: Vec<u16>,
    pub paid: Vec<u128>,
}
impl AmmV4Sharing {
    pub const SPACE: usize = 8 + 2 * 32 + 8 + 16 + 3 * 4 + MAX_RECIPIENTS * (32 + 2 + 16);
}

fn valid_shares(basis_points: &[u16]) -> Result<()> {
    require!(
        !basis_points.is_empty()
            && basis_points.len() <= MAX_RECIPIENTS
            && basis_points.iter().all(|value| *value > 0)
            && basis_points
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>()
                == 10_000,
        SharingError::InvalidPlan
    );
    Ok(())
}

/// Cumulative floor targets avoid giving a privileged last recipient all
/// repeated one-lamport deposits. Unallocated rounding dust stays in custody.
pub fn payments(
    received: u128,
    basis_points: &[u16],
    paid: &[u128],
) -> Result<(Vec<u128>, Vec<u64>)> {
    valid_shares(basis_points)?;
    require_eq!(basis_points.len(), paid.len(), SharingError::InvalidPlan);
    let mut targets = Vec::with_capacity(paid.len());
    let mut due = Vec::with_capacity(paid.len());
    for (bps, previous) in basis_points.iter().zip(paid) {
        let target = received
            .checked_mul(u128::from(*bps))
            .ok_or_else(|| error!(SharingError::Overflow))?
            / 10_000;
        let amount = target
            .checked_sub(*previous)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| error!(SharingError::InvalidPlan))?;
        targets.push(target);
        due.push(amount);
    }
    Ok((targets, due))
}

#[derive(Accounts)]
#[instruction(recipients: Vec<Pubkey>, basis_points: Vec<u16>, revision: u64)]
pub struct CreateAmmV4Sharing<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump, has_one = creator)]
    pub rights: Account<'info, AmmV4Rights>,
    #[account(init, payer = creator, space = AmmV4Sharing::SPACE,
        seeds = [SHARING_SEED, rights.pool.as_ref(), &revision.to_le_bytes()], bump)]
    pub plan: Account<'info, AmmV4Sharing>,
    pub system_program: Program<'info, System>,
}
pub fn create(
    ctx: Context<CreateAmmV4Sharing>,
    recipients: Vec<Pubkey>,
    basis_points: Vec<u16>,
    revision: u64,
) -> Result<()> {
    valid_shares(&basis_points)?;
    require_eq!(
        recipients.len(),
        basis_points.len(),
        SharingError::InvalidPlan
    );
    for (i, recipient) in recipients.iter().enumerate() {
        require!(
            *recipient != Pubkey::default()
                && *recipient != ctx.accounts.plan.key()
                && !recipients[i + 1..].contains(recipient),
            SharingError::InvalidPlan
        );
    }
    let next = ctx.accounts.rights.next_revision(revision)?;
    ctx.accounts.plan.set_inner(AmmV4Sharing {
        pool: ctx.accounts.rights.pool,
        creator: ctx.accounts.creator.key(),
        creation_revision: revision,
        distributed: 0,
        paid: vec![0; recipients.len()],
        recipients,
        basis_points,
    });
    ctx.accounts.rights.creator_payout = ctx.accounts.plan.key();
    ctx.accounts.rights.revision = next;
    Ok(())
}

#[derive(Accounts)]
pub struct SelectAmmV4CreatorWallet<'info> {
    pub creator: Signer<'info>,
    pub wallet: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump, has_one = creator)]
    pub rights: Account<'info, AmmV4Rights>,
}
pub fn select_wallet(ctx: Context<SelectAmmV4CreatorWallet>, revision: u64) -> Result<()> {
    let next = ctx.accounts.rights.next_revision(revision)?;
    ctx.accounts.rights.creator_payout = ctx.accounts.wallet.key();
    ctx.accounts.rights.revision = next;
    Ok(())
}

#[derive(Accounts)]
pub struct DistributeAmmV4Sharing<'info> {
    #[account(mut, seeds = [SHARING_SEED, plan.pool.as_ref(), &plan.creation_revision.to_le_bytes()], bump)]
    pub plan: Account<'info, AmmV4Sharing>,
}
pub fn distribute(ctx: Context<DistributeAmmV4Sharing>) -> Result<()> {
    let plan = &mut ctx.accounts.plan;
    require_eq!(
        ctx.remaining_accounts.len(),
        plan.recipients.len(),
        SharingError::InvalidRecipients
    );
    let paid_total = plan
        .paid
        .iter()
        .try_fold(0u128, |sum, n| sum.checked_add(*n))
        .ok_or_else(|| error!(SharingError::Overflow))?;
    require_eq!(paid_total, plan.distributed, SharingError::InvalidPlan);
    let info = plan.to_account_info();
    let rent = Rent::get()?.minimum_balance(AmmV4Sharing::SPACE);
    let available = info
        .lamports()
        .checked_sub(rent)
        .ok_or_else(|| error!(SharingError::InvalidBalance))?;
    let received = plan
        .distributed
        .checked_add(u128::from(available))
        .ok_or_else(|| error!(SharingError::Overflow))?;
    let (targets, due) = payments(received, &plan.basis_points, &plan.paid)?;
    let sum_due = due
        .iter()
        .try_fold(0u64, |sum, n| sum.checked_add(*n))
        .filter(|sum| *sum <= available)
        .ok_or_else(|| error!(SharingError::InvalidBalance))?;
    let next_distributed = plan
        .distributed
        .checked_add(u128::from(sum_due))
        .ok_or_else(|| error!(SharingError::Overflow))?;
    let mut balances = Vec::with_capacity(due.len());
    for ((recipient, expected), amount) in ctx
        .remaining_accounts
        .iter()
        .zip(&plan.recipients)
        .zip(&due)
    {
        require!(
            *recipient.key == *expected && recipient.is_writable && recipient.key != info.key,
            SharingError::InvalidRecipients
        );
        balances.push(
            recipient
                .lamports()
                .checked_add(*amount)
                .ok_or_else(|| error!(SharingError::Overflow))?,
        );
    }
    // Acquire all borrows before modifying any balance, including on native host tests.
    let mut source = info.try_borrow_mut_lamports()?;
    let mut destinations = Vec::with_capacity(due.len());
    for recipient in ctx.remaining_accounts {
        destinations.push(recipient.try_borrow_mut_lamports()?);
    }
    **source -= sum_due;
    for (target, balance) in destinations.iter_mut().zip(balances) {
        ***target = balance;
    }
    plan.paid = targets;
    plan.distributed = next_distributed;
    emit!(AmmV4SharingDistribution {
        plan: info.key(),
        amount: sum_due,
        distributed: next_distributed
    });
    Ok(())
}

#[event]
pub struct AmmV4SharingDistribution {
    pub plan: Pubkey,
    pub amount: u64,
    pub distributed: u128,
}
#[error_code(offset = 7800)]
pub enum SharingError {
    #[msg("Invalid immutable creator sharing plan")]
    InvalidPlan,
    #[msg("Sharing payout accounts do not match the plan")]
    InvalidRecipients,
    #[msg("Sharing arithmetic overflow")]
    Overflow,
    #[msg("Sharing account must preserve rent and cover all payouts")]
    InvalidBalance,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_shares_are_independent_of_deposit_batching_and_preserve_dust() {
        for bps in [
            vec![6000, 4000],
            vec![3333, 3333, 3334],
            vec![1, 9999],
            vec![10000],
        ] {
            let mut paid = vec![0u128; bps.len()];
            let mut distributed = 0u128;
            for received in 0..=20_000 {
                let (targets, due) = payments(received, &bps, &paid).unwrap();
                let sum: u64 = due.iter().sum();
                distributed += u128::from(sum);
                paid = targets;
                assert_eq!(paid.iter().sum::<u128>(), distributed);
                assert!(received - distributed < bps.len() as u128);
                assert_eq!(
                    payments(received, &bps, &paid).unwrap().1,
                    vec![0; bps.len()]
                );
            }
            let single = payments(20_000, &bps, &vec![0; bps.len()]).unwrap().0;
            assert_eq!(paid, single);
        }
    }
    #[test]
    fn sharing_rejects_invalid_weights_lengths_and_overflows() {
        for bps in [
            vec![],
            vec![0, 10000],
            vec![9999],
            vec![10001],
            vec![1250; 9],
        ] {
            assert!(payments(100, &bps, &vec![0; bps.len()]).is_err());
        }
        assert!(payments(1, &[10000], &[]).is_err());
        assert!(payments(1, &[10000], &[2]).is_err());
        assert!(payments(u128::MAX, &[10000], &[0]).is_err());
    }
}
