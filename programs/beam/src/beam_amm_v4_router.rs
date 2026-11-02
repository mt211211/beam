//! Source-pinned orderbook-free AMM v4 SwapBaseInV2 route (tag 16).
//! Source: raydium-amm 91739d8d9f5cf347e0ce14ebe4d2bb7069fb31b6,
//! program/src/instruction.rs::swap_base_in_v2.
//! User-selected creator payouts are funded from the community allocation.
use crate::{beam_amm_v4, beam_amm_v4_companion::*, Global};
use anchor_lang::system_program::{self, Transfer};
use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke,
    },
};
use anchor_spl::token::{Token, TokenAccount};

pub const POLICY_SEED: &[u8] = b"beam-amm-fee-policy";
pub const COMMUNITY: u8 = 2;
pub const TREASURY: u8 = 3;
pub const FOUNDER: u8 = 4;

#[account]
pub struct AmmV4RoutingPolicy {
    pub pool: Pubkey,
    pub community: Pubkey,
    pub treasury: Pubkey,
    pub founder: Pubkey,
    pub creator_bps: u16,
    pub enabled: bool,
}
impl AmmV4RoutingPolicy {
    pub const SPACE: usize = 8 + 4 * 32 + 2 + 1;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub total: u64,
    pub creator: u64,
    pub community: u64,
    pub treasury: u64,
    pub founder: u64,
}
pub fn allocation(notional: u64, creator_bps: u16) -> Result<Allocation> {
    require!(creator_bps <= 42, RouteError::InvalidPolicy);
    let split = crate::beam_economics::allocate(notional)?;
    let creator = crate::fee_math::fee(notional, u64::from(creator_bps))?;
    let community = split
        .community
        .checked_sub(creator)
        .ok_or_else(|| error!(RouteError::InvalidPolicy))?;
    Ok(Allocation {
        total: split.total,
        creator,
        community,
        treasury: split.treasury,
        founder: split.founder,
    })
}

#[derive(Accounts)]
pub struct ConfigureAmmV4Routing<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub creator: Signer<'info>,
    #[account(seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump, has_one = creator)]
    pub rights: Account<'info, AmmV4Rights>,
    #[account(init, payer = authority, space = AmmV4RoutingPolicy::SPACE,
              seeds = [POLICY_SEED, rights.pool.as_ref()], bump)]
    pub policy: Account<'info, AmmV4RoutingPolicy>,
    pub system_program: Program<'info, System>,
}
pub fn configure(
    ctx: Context<ConfigureAmmV4Routing>,
    community: Pubkey,
    treasury: Pubkey,
    founder: Pubkey,
    creator_bps: u16,
    revision: u64,
) -> Result<()> {
    ctx.accounts.rights.check_revision(revision)?;
    require!(
        creator_bps <= 42
            && [community, treasury, founder]
                .iter()
                .all(|key| *key != Pubkey::default()),
        RouteError::InvalidPolicy
    );
    ctx.accounts.policy.set_inner(AmmV4RoutingPolicy {
        pool: ctx.accounts.rights.pool,
        community,
        treasury,
        founder,
        creator_bps,
        enabled: false,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct EnableAmmV4Routing<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    pub authority: Signer<'info>,
    pub creator: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump, has_one = creator)]
    pub rights: Account<'info, AmmV4Rights>,
    #[account(mut, seeds = [POLICY_SEED, rights.pool.as_ref()], bump)]
    pub policy: Account<'info, AmmV4RoutingPolicy>,
}
pub fn set_enabled(ctx: Context<EnableAmmV4Routing>, enabled: bool, revision: u64) -> Result<()> {
    ctx.accounts.rights.check_revision(revision)?;
    ctx.accounts.policy.enabled = enabled;
    Ok(())
}

/// Updates apply only to future accrual, advance the user-visible revision,
/// and disable routing until authority and creator explicitly re-enable it.
pub fn update_policy(
    ctx: Context<EnableAmmV4Routing>,
    community: Pubkey,
    treasury: Pubkey,
    founder: Pubkey,
    creator_bps: u16,
    revision: u64,
) -> Result<()> {
    require!(
        creator_bps <= 42
            && [community, treasury, founder]
                .iter()
                .all(|key| *key != Pubkey::default()),
        RouteError::InvalidPolicy
    );
    let next = ctx.accounts.rights.next_revision(revision)?;
    let policy = &mut ctx.accounts.policy;
    policy.community = community;
    policy.treasury = treasury;
    policy.founder = founder;
    policy.creator_bps = creator_bps;
    policy.enabled = false;
    ctx.accounts.rights.revision = next;
    emit!(AmmV4PolicyChanged {
        pool: policy.pool,
        community,
        treasury,
        founder,
        creator_bps,
        revision: next
    });
    Ok(())
}
#[event]
pub struct AmmV4PolicyChanged {
    pub pool: Pubkey,
    pub community: Pubkey,
    pub treasury: Pubkey,
    pub founder: Pubkey,
    pub creator_bps: u16,
    pub revision: u64,
}

#[derive(Accounts)]
pub struct SwapAmmV4<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump)]
    pub rights: Box<Account<'info, AmmV4Rights>>,
    #[account(seeds = [POLICY_SEED, rights.pool.as_ref()], bump)]
    pub policy: Box<Account<'info, AmmV4RoutingPolicy>>,
    /// CHECK: Canonical identity is revalidated immediately before CPI.
    #[account(mut, address = rights.pool, owner = AMM_PROGRAM)]
    pub pool: UncheckedAccount<'info>,
    /// CHECK: Pinned AMM v4 program; this account must be executable.
    #[account(address = AMM_PROGRAM, executable)]
    pub raydium_program: UncheckedAccount<'info>,
    /// CHECK: Canonical authority PDA is checked before CPI.
    pub amm_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub coin_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub pc_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, constraint = source.owner == user.key() @ RouteError::InvalidTokenAccount)]
    pub source: Box<Account<'info, TokenAccount>>,
    #[account(mut, constraint = destination.owner == user.key() @ RouteError::InvalidTokenAccount)]
    pub destination: Box<Account<'info, TokenAccount>>,
    #[account(init_if_needed, payer = user, space = AmmV4Vault::SPACE,
        seeds = [VAULT_SEED, rights.pool.as_ref(), &[CREATOR], rights.creator_payout.as_ref()], bump)]
    pub creator_vault: Box<Account<'info, AmmV4Vault>>,
    #[account(init_if_needed, payer = user, space = AmmV4Vault::SPACE,
        seeds = [VAULT_SEED, rights.pool.as_ref(), &[COMMUNITY], policy.community.as_ref()], bump)]
    pub community_vault: Box<Account<'info, AmmV4Vault>>,
    #[account(init_if_needed, payer = user, space = AmmV4Vault::SPACE,
        seeds = [VAULT_SEED, rights.pool.as_ref(), &[TREASURY], policy.treasury.as_ref()], bump)]
    pub treasury_vault: Box<Account<'info, AmmV4Vault>>,
    #[account(init_if_needed, payer = user, space = AmmV4Vault::SPACE,
        seeds = [VAULT_SEED, rights.pool.as_ref(), &[FOUNDER], policy.founder.as_ref()], bump)]
    pub founder_vault: Box<Account<'info, AmmV4Vault>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Eight accounts, tag 16, then exact u64 input/minimum output. No orderbook
/// accounts, transfer authority delegation, LP accounts, or PumpSwap fallback.
pub fn swap_instruction(keys: [Pubkey; 8], amount_in: u64, minimum_out: u64) -> Instruction {
    let mut data = vec![16];
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_out.to_le_bytes());
    let accounts = keys
        .into_iter()
        .enumerate()
        .map(|(index, pubkey)| {
            if matches!(index, 1 | 3 | 4 | 5 | 6) {
                AccountMeta::new(pubkey, false)
            } else {
                AccountMeta::new_readonly(pubkey, index == 7)
            }
        })
        .collect();
    Instruction {
        program_id: AMM_PROGRAM,
        accounts,
        data,
    }
}

fn validate_vault(
    vault: &AmmV4Vault,
    info: &AccountInfo,
    pool: Pubkey,
    beneficiary: Pubkey,
    kind: u8,
    rent: u64,
) -> Result<()> {
    if vault.pool == Pubkey::default() {
        require!(
            vault.credited == 0 && vault.claimed == 0,
            RouteError::InvalidVault
        );
    } else {
        require!(
            vault.pool == pool && vault.beneficiary == beneficiary && vault.kind == kind,
            RouteError::InvalidVault
        );
    }
    let required = rent
        .checked_add(vault.outstanding()?)
        .ok_or_else(|| error!(RouteError::Overflow))?;
    require!(info.lamports() >= required, RouteError::InvalidVault);
    Ok(())
}

fn credit<'info>(
    vault: &mut AmmV4Vault,
    info: AccountInfo<'info>,
    user: AccountInfo<'info>,
    system: AccountInfo<'info>,
    pool: Pubkey,
    beneficiary: Pubkey,
    kind: u8,
    amount: u64,
) -> Result<()> {
    let credited = vault.next_credit(amount)?;
    if amount > 0 {
        system_program::transfer(
            CpiContext::new(
                system,
                Transfer {
                    from: user,
                    to: info,
                },
            ),
            amount,
        )?;
    }
    vault.pool = pool;
    vault.beneficiary = beneficiary;
    vault.kind = kind;
    vault.credited = credited;
    Ok(())
}

pub fn swap(
    ctx: Context<SwapAmmV4>,
    amount_in: u64,
    minimum_out: u64,
    maximum_beam_fee: u64,
    revision: u64,
) -> Result<()> {
    let a = &mut *ctx.accounts;
    a.rights.check_revision(revision)?;
    require!(!a.rights.paused && a.policy.enabled, RouteError::Disabled);
    require!(amount_in > 0 && minimum_out > 0, RouteError::InvalidAmount);
    require_keys_eq!(a.policy.pool, a.rights.pool, RouteError::InvalidPolicy);
    validate_pool_identity(
        a.pool.key(),
        *a.pool.owner,
        &a.pool.try_borrow_data()?,
        a.rights.mint,
        a.rights.market_seed,
    )?;
    let p = beam_amm_v4::derive_pool_addresses(&AMM_PROGRAM, &a.rights.market_seed)
        .map_err(|_| error!(RouteError::InvalidPool))?;
    require_keys_eq!(a.amm_authority.key(), p.authority, RouteError::InvalidPool);
    require_keys_eq!(a.coin_vault.key(), p.coin_vault, RouteError::InvalidPool);
    require_keys_eq!(a.pc_vault.key(), p.pc_vault, RouteError::InvalidPool);
    require!(
        a.coin_vault.mint == a.rights.mint
            && a.coin_vault.owner == p.authority
            && a.pc_vault.mint == anchor_spl::token::spl_token::native_mint::ID
            && a.pc_vault.owner == p.authority,
        RouteError::InvalidTokenAccount
    );
    let quote = anchor_spl::token::spl_token::native_mint::ID;
    let is_buy = a.source.mint == quote && a.destination.mint == a.rights.mint;
    require!(
        is_buy || (a.source.mint == a.rights.mint && a.destination.mint == quote),
        RouteError::InvalidTokenAccount
    );
    let keys = [
        a.pool.key(),
        a.coin_vault.key(),
        a.pc_vault.key(),
        a.source.key(),
        a.destination.key(),
        a.creator_vault.key(),
        a.community_vault.key(),
        a.treasury_vault.key(),
        a.founder_vault.key(),
        a.user.key(),
    ];
    for (index, key) in keys.iter().enumerate() {
        require!(
            !keys[index + 1..].contains(key),
            RouteError::AliasedAccounts
        );
    }
    let rent = Rent::get()?.minimum_balance(AmmV4Vault::SPACE);
    for (vault, info, beneficiary, kind) in [
        (
            &**a.creator_vault,
            a.creator_vault.to_account_info(),
            a.rights.creator_payout,
            CREATOR,
        ),
        (
            &**a.community_vault,
            a.community_vault.to_account_info(),
            a.policy.community,
            COMMUNITY,
        ),
        (
            &**a.treasury_vault,
            a.treasury_vault.to_account_info(),
            a.policy.treasury,
            TREASURY,
        ),
        (
            &**a.founder_vault,
            a.founder_vault.to_account_info(),
            a.policy.founder,
            FOUNDER,
        ),
    ] {
        validate_vault(vault, &info, a.rights.pool, beneficiary, kind, rent)?;
    }
    let source_before = a.source.amount;
    let destination_before = a.destination.amount;
    require!(source_before >= amount_in, RouteError::InvalidAmount);
    // Quote-side fee is paid in native SOL alongside the WSOL swap. It never
    // debits pool reserves or silently closes/unwraps the user's token account.
    if is_buy {
        require!(
            allocation(amount_in, a.policy.creator_bps)?.total <= maximum_beam_fee,
            RouteError::MaximumFee
        );
    }
    let ix = swap_instruction(
        [
            a.token_program.key(),
            a.pool.key(),
            a.amm_authority.key(),
            a.coin_vault.key(),
            a.pc_vault.key(),
            a.source.key(),
            a.destination.key(),
            a.user.key(),
        ],
        amount_in,
        minimum_out,
    );
    invoke(
        &ix,
        &[
            a.token_program.to_account_info(),
            a.pool.to_account_info(),
            a.amm_authority.to_account_info(),
            a.coin_vault.to_account_info(),
            a.pc_vault.to_account_info(),
            a.source.to_account_info(),
            a.destination.to_account_info(),
            a.user.to_account_info(),
            a.raydium_program.to_account_info(),
        ],
    )?;
    a.source.reload()?;
    a.destination.reload()?;
    let output = a
        .destination
        .amount
        .checked_sub(destination_before)
        .ok_or_else(|| error!(RouteError::InvalidResult))?;
    require!(
        source_before.checked_sub(a.source.amount) == Some(amount_in) && output >= minimum_out,
        RouteError::InvalidResult
    );
    let notional = if is_buy { amount_in } else { output };
    let fees = allocation(notional, a.policy.creator_bps)?;
    require!(fees.total <= maximum_beam_fee, RouteError::MaximumFee);
    let pool = a.rights.pool;
    let user = a.user.to_account_info();
    let system = a.system_program.to_account_info();
    let creator_info = a.creator_vault.to_account_info();
    let community_info = a.community_vault.to_account_info();
    let treasury_info = a.treasury_vault.to_account_info();
    let founder_info = a.founder_vault.to_account_info();
    credit(
        &mut a.creator_vault,
        creator_info,
        user.clone(),
        system.clone(),
        pool,
        a.rights.creator_payout,
        CREATOR,
        fees.creator,
    )?;
    credit(
        &mut a.community_vault,
        community_info,
        user.clone(),
        system.clone(),
        pool,
        a.policy.community,
        COMMUNITY,
        fees.community,
    )?;
    credit(
        &mut a.treasury_vault,
        treasury_info,
        user.clone(),
        system.clone(),
        pool,
        a.policy.treasury,
        TREASURY,
        fees.treasury,
    )?;
    credit(
        &mut a.founder_vault,
        founder_info,
        user,
        system,
        pool,
        a.policy.founder,
        FOUNDER,
        fees.founder,
    )?;
    emit!(AmmV4RoutedSwap {
        pool,
        user: a.user.key(),
        is_buy,
        amount_in,
        amount_out: output,
        quote_notional: notional,
        total_beam_fee: fees.total,
        creator: fees.creator,
        community: fees.community,
        treasury: fees.treasury,
        founder: fees.founder,
        revision
    });
    Ok(())
}

#[event]
pub struct AmmV4RoutedSwap {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub is_buy: bool,
    pub amount_in: u64,
    pub amount_out: u64,
    pub quote_notional: u64,
    pub total_beam_fee: u64,
    pub creator: u64,
    pub community: u64,
    pub treasury: u64,
    pub founder: u64,
    pub revision: u64,
}

#[error_code(offset = 7600)]
pub enum RouteError {
    #[msg("Invalid fixed AMM v4 fee policy")]
    InvalidPolicy,
    #[msg("AMM v4 routing is disabled")]
    Disabled,
    #[msg("Input and minimum output must be positive")]
    InvalidAmount,
    #[msg("Invalid AMM v4 pool")]
    InvalidPool,
    #[msg("Invalid owned classic-SPL token account")]
    InvalidTokenAccount,
    #[msg("Swap/vault accounts must not alias")]
    AliasedAccounts,
    #[msg("Invalid or insolvent companion vault")]
    InvalidVault,
    #[msg("Beam fee exceeds the user's explicit maximum")]
    MaximumFee,
    #[msg("Raydium returned unexpected balance changes")]
    InvalidResult,
    #[msg("AMM v4 route accounting overflow")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creator_comes_from_community_without_an_additional_charge() {
        let a = allocation(100_000_000_000, 30).unwrap();
        assert_eq!(
            a,
            Allocation {
                total: 500_000_000,
                creator: 300_000_000,
                community: 120_000_000,
                treasury: 50_000_000,
                founder: 30_000_000
            }
        );
        assert_eq!(
            allocation(1, 30).unwrap(),
            Allocation {
                total: 1,
                creator: 1,
                community: 0,
                treasury: 0,
                founder: 0
            }
        );
        assert!(allocation(1000, 43).is_err());
        for notional in (0..=100_000).chain([u64::MAX]) {
            for bps in [0, 1, 30, 42] {
                let a = allocation(notional, bps).unwrap();
                assert_eq!(a.total, a.creator + a.community + a.treasury + a.founder);
                assert!((a.treasury as u128) * 10_000 <= (notional as u128) * 5);
                assert!((a.founder as u128) * 10_000 <= (notional as u128) * 3);
                assert!(a.creator <= crate::beam_economics::allocate(notional).unwrap().community);
            }
        }
    }
    #[test]
    fn swap_v2_bytes_and_account_privileges_match_the_pinned_source() {
        let keys = std::array::from_fn(|i| Pubkey::new_from_array([i as u8 + 1; 32]));
        for (input, minimum) in [(1, 1), (u64::MAX, u64::MAX)] {
            let ix = swap_instruction(keys, input, minimum);
            assert_eq!(ix.program_id, AMM_PROGRAM);
            assert_eq!(ix.data.len(), 17);
            assert_eq!(ix.data[0], 16);
            assert_eq!(&ix.data[1..9], &input.to_le_bytes());
            assert_eq!(&ix.data[9..17], &minimum.to_le_bytes());
            assert_eq!(ix.accounts.len(), 8);
            for (i, meta) in ix.accounts.iter().enumerate() {
                assert_eq!(meta.pubkey, keys[i]);
                assert_eq!(meta.is_signer, i == 7);
                assert_eq!(meta.is_writable, matches!(i, 1 | 3 | 4 | 5 | 6));
            }
        }
    }
}
