//! Beam-owned native SOL companion custody; no Raydium principal or LP access.
//! Explicit deposits only. No trade observation or automatic fee collection.
use crate::{beam_amm_v4, trade_types::BondingCurve, Global};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::token::Mint;

pub const RIGHTS_SEED: &[u8] = b"beam-amm-rights";
pub const VAULT_SEED: &[u8] = b"beam-amm-vault";
pub const DONATION_SEED: &[u8] = b"beam-donation";
pub const CREATOR: u8 = 0;
pub const DONATION: u8 = 1;

#[cfg(feature = "raydium-devnet")]
pub const AMM_PROGRAM: Pubkey = beam_amm_v4::DEVNET_PROGRAM_ID;
#[cfg(not(feature = "raydium-devnet"))]
pub const AMM_PROGRAM: Pubkey = beam_amm_v4::MAINNET_PROGRAM_ID;

#[account]
#[derive(Debug)]
pub struct AmmV4Rights {
    pub pool: Pubkey,
    pub mint: Pubkey,
    pub market_seed: Pubkey,
    pub creator: Pubkey,
    pub creator_payout: Pubkey,
    pub donation_record: Pubkey,
    pub revision: u64,
    pub paused: bool,
}
impl AmmV4Rights {
    pub const SPACE: usize = 8 + 6 * 32 + 8 + 1;

    pub(crate) fn check_revision(&self, expected: u64) -> Result<()> {
        require_eq!(self.revision, expected, CompanionError::StaleRevision);
        Ok(())
    }

    pub(crate) fn next_revision(&self, expected: u64) -> Result<u64> {
        self.check_revision(expected)?;
        self.revision
            .checked_add(1)
            .ok_or_else(|| error!(CompanionError::Overflow))
    }

    pub fn successor(
        &mut self,
        authority: Pubkey,
        current: Pubkey,
        next: Pubkey,
        expected_authority: Pubkey,
        expected_revision: u64,
    ) -> Result<()> {
        require_keys_eq!(authority, expected_authority, CompanionError::Unauthorized);
        require_keys_eq!(current, self.creator, CompanionError::Unauthorized);
        require!(
            next != Pubkey::default() && next != current,
            CompanionError::InvalidRecipient
        );
        let revision = self.next_revision(expected_revision)?;
        self.creator = next;
        self.creator_payout = next;
        // A successor must actively opt into any donation plan. Historical claims
        // remain in their beneficiary vaults regardless of this selection.
        self.donation_record = Pubkey::default();
        self.revision = revision;
        Ok(())
    }

    fn beneficiary(
        &self,
        kind: u8,
        expected: u64,
        donation: Option<(Pubkey, &DonationRecipient)>,
    ) -> Result<Pubkey> {
        self.check_revision(expected)?;
        require!(!self.paused, CompanionError::Paused);
        match kind {
            CREATOR => Ok(self.creator_payout),
            DONATION => {
                let (key, record) =
                    donation.ok_or_else(|| error!(CompanionError::InvalidRecipient))?;
                require_keys_eq!(key, self.donation_record, CompanionError::InvalidRecipient);
                require!(record.active, CompanionError::RecipientRevoked);
                Ok(record.recipient)
            }
            _ => err!(CompanionError::InvalidKind),
        }
    }
}

#[account]
pub struct DonationRecipient {
    pub recipient: Pubkey,
    pub active: bool,
}
impl DonationRecipient {
    pub const SPACE: usize = 8 + 32 + 1;
}

#[account]
#[derive(Debug)]
pub struct AmmV4Vault {
    pub pool: Pubkey,
    pub beneficiary: Pubkey,
    pub kind: u8,
    pub credited: u64,
    pub claimed: u64,
}
impl AmmV4Vault {
    pub const SPACE: usize = 8 + 2 * 32 + 1 + 2 * 8;

    pub(crate) fn outstanding(&self) -> Result<u64> {
        self.credited
            .checked_sub(self.claimed)
            .ok_or_else(|| error!(CompanionError::Insolvent))
    }

    pub(crate) fn next_credit(&self, amount: u64) -> Result<u64> {
        self.credited
            .checked_add(amount)
            .ok_or_else(|| error!(CompanionError::Overflow))
    }
}

#[derive(Accounts)]
pub struct RegisterAmmV4Rights<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub creator: Signer<'info>,
    pub mint: Account<'info, Mint>,
    #[account(seeds = [b"bonding-curve", mint.key().as_ref()], bump)]
    pub bonding_curve: Box<Account<'info, BondingCurve>>,
    /// CHECK: Canonical source-pinned AMM v4 pool validated by register().
    pub pool: UncheckedAccount<'info>,
    #[account(init, payer = authority, space = AmmV4Rights::SPACE,
              seeds = [RIGHTS_SEED, pool.key().as_ref()], bump)]
    pub rights: Account<'info, AmmV4Rights>,
    pub system_program: Program<'info, System>,
}

/// Read only identity fields needed by this companion. This deliberately
/// does not duplicate the post-CPI settlement snapshot owned by PR #34.
pub fn validate_pool_identity(
    pool: Pubkey,
    owner: Pubkey,
    data: &[u8],
    mint: Pubkey,
    market_seed: Pubkey,
) -> Result<()> {
    require_keys_eq!(owner, AMM_PROGRAM, CompanionError::InvalidPool);
    require_keys_neq!(
        mint,
        anchor_spl::token::spl_token::native_mint::ID,
        CompanionError::InvalidPool
    );
    require_eq!(data.len(), 752, CompanionError::InvalidPool);
    let addresses = beam_amm_v4::derive_pool_addresses(&owner, &market_seed)
        .map_err(|_| error!(CompanionError::InvalidPool))?;
    require_keys_eq!(pool, addresses.amm, CompanionError::InvalidPool);
    let word = |offset| u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
    require!(matches!(word(0), 1 | 6), CompanionError::InvalidPool);
    require_eq!(
        word(8),
        u64::from(addresses.nonce),
        CompanionError::InvalidPool
    );
    for (offset, key) in [
        (336, addresses.coin_vault),
        (368, addresses.pc_vault),
        (400, mint),
        (432, anchor_spl::token::spl_token::native_mint::ID),
        (464, addresses.lp_mint),
        (528, market_seed),
        (592, addresses.target_orders),
    ] {
        require!(
            data[offset..offset + 32] == key.to_bytes(),
            CompanionError::InvalidPool
        );
    }
    Ok(())
}

pub fn register(ctx: Context<RegisterAmmV4Rights>, market_seed: Pubkey) -> Result<()> {
    let curve = &ctx.accounts.bonding_curve;
    require!(
        curve.complete && !curve.is_mayhem_mode,
        CompanionError::InvalidCurve
    );
    require!(
        curve.quote_mint == Pubkey::default()
            || curve.quote_mint == anchor_spl::token::spl_token::native_mint::ID,
        CompanionError::InvalidCurve
    );
    require_keys_eq!(
        curve.creator,
        ctx.accounts.creator.key(),
        CompanionError::Unauthorized
    );
    validate_pool_identity(
        ctx.accounts.pool.key(),
        *ctx.accounts.pool.owner,
        &ctx.accounts.pool.try_borrow_data()?,
        ctx.accounts.mint.key(),
        market_seed,
    )?;
    ctx.accounts.rights.set_inner(AmmV4Rights {
        pool: ctx.accounts.pool.key(),
        mint: ctx.accounts.mint.key(),
        market_seed,
        creator: ctx.accounts.creator.key(),
        creator_payout: ctx.accounts.creator.key(),
        donation_record: Pubkey::default(),
        revision: 0,
        paused: false,
    });
    emit!(CompanionRightsChanged {
        pool: ctx.accounts.rights.pool,
        creator: ctx.accounts.rights.creator,
        revision: 0
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ChangeAmmV4Creator<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    pub authority: Signer<'info>,
    pub creator: Signer<'info>,
    pub successor: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump)]
    pub rights: Account<'info, AmmV4Rights>,
}

pub fn change_creator(ctx: Context<ChangeAmmV4Creator>, expected_revision: u64) -> Result<()> {
    ctx.accounts.rights.successor(
        ctx.accounts.authority.key(),
        ctx.accounts.creator.key(),
        ctx.accounts.successor.key(),
        ctx.accounts.global.authority,
        expected_revision,
    )?;
    emit!(CompanionRightsChanged {
        pool: ctx.accounts.rights.pool,
        creator: ctx.accounts.rights.creator,
        revision: ctx.accounts.rights.revision
    });
    Ok(())
}

/// Administrative CTO changes future rights only. The existing dedicated
/// Global admin_set_creator_authority role authorizes the takeover, and the
/// successor must consent. Historical creator/sharing balances are untouched.
#[derive(Accounts)]
pub struct TakeoverAmmV4Creator<'info> {
    #[account(seeds = [b"global"], bump, has_one = admin_set_creator_authority)]
    pub global: Box<Account<'info, Global>>,
    pub admin_set_creator_authority: Signer<'info>,
    pub successor: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump)]
    pub rights: Account<'info, AmmV4Rights>,
}
pub fn takeover(
    ctx: Context<TakeoverAmmV4Creator>,
    expected_creator: Pubkey,
    revision: u64,
) -> Result<()> {
    ctx.accounts.rights.successor(
        ctx.accounts.admin_set_creator_authority.key(),
        expected_creator,
        ctx.accounts.successor.key(),
        ctx.accounts.global.admin_set_creator_authority,
        revision,
    )?;
    emit!(CompanionTakeover {
        pool: ctx.accounts.rights.pool,
        authority: ctx.accounts.admin_set_creator_authority.key(),
        previous_creator: expected_creator,
        creator: ctx.accounts.rights.creator,
        revision: ctx.accounts.rights.revision
    });
    Ok(())
}
#[event]
pub struct CompanionTakeover {
    pub pool: Pubkey,
    pub authority: Pubkey,
    pub previous_creator: Pubkey,
    pub creator: Pubkey,
    pub revision: u64,
}

#[derive(Accounts)]
pub struct RegisterDonationRecipient<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub recipient: Signer<'info>,
    #[account(init, payer = authority, space = DonationRecipient::SPACE,
              seeds = [DONATION_SEED, recipient.key().as_ref()], bump)]
    pub record: Account<'info, DonationRecipient>,
    pub system_program: Program<'info, System>,
}

pub fn register_donation(ctx: Context<RegisterDonationRecipient>) -> Result<()> {
    require!(
        ctx.accounts.recipient.key() != Pubkey::default(),
        CompanionError::InvalidRecipient
    );
    ctx.accounts.record.set_inner(DonationRecipient {
        recipient: ctx.accounts.recipient.key(),
        active: true,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RevokeDonationRecipient<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    pub authority: Signer<'info>,
    #[account(mut, seeds = [DONATION_SEED, record.recipient.as_ref()], bump)]
    pub record: Account<'info, DonationRecipient>,
}

pub fn revoke_donation(ctx: Context<RevokeDonationRecipient>) -> Result<()> {
    ctx.accounts.record.active = false;
    Ok(())
}

#[derive(Accounts)]
pub struct SelectAmmV4Donation<'info> {
    pub creator: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump, has_one = creator)]
    pub rights: Account<'info, AmmV4Rights>,
    #[account(seeds = [DONATION_SEED, record.recipient.as_ref()], bump)]
    pub record: Account<'info, DonationRecipient>,
}

pub fn select_donation(ctx: Context<SelectAmmV4Donation>, expected_revision: u64) -> Result<()> {
    require!(ctx.accounts.record.active, CompanionError::RecipientRevoked);
    let revision = ctx.accounts.rights.next_revision(expected_revision)?;
    ctx.accounts.rights.donation_record = ctx.accounts.record.key();
    ctx.accounts.rights.revision = revision;
    Ok(())
}

#[derive(Accounts)]
pub struct PauseAmmV4Rights<'info> {
    #[account(seeds = [b"global"], bump, has_one = authority)]
    pub global: Box<Account<'info, Global>>,
    pub authority: Signer<'info>,
    #[account(mut, seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump)]
    pub rights: Account<'info, AmmV4Rights>,
}

pub fn pause(ctx: Context<PauseAmmV4Rights>, paused: bool, expected_revision: u64) -> Result<()> {
    let revision = ctx.accounts.rights.next_revision(expected_revision)?;
    ctx.accounts.rights.paused = paused;
    ctx.accounts.rights.revision = revision;
    Ok(())
}

#[derive(Accounts)]
#[instruction(kind: u8)]
pub struct FundAmmV4Vault<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [RIGHTS_SEED, rights.pool.as_ref()], bump)]
    pub rights: Account<'info, AmmV4Rights>,
    /// CHECK: Exact beneficiary is validated against the rights/registry before CPI.
    pub beneficiary: UncheckedAccount<'info>,
    pub donation_record: Option<Account<'info, DonationRecipient>>,
    #[account(init_if_needed, payer = payer, space = AmmV4Vault::SPACE,
              seeds = [VAULT_SEED, rights.pool.as_ref(), &[kind], beneficiary.key().as_ref()], bump)]
    pub vault: Account<'info, AmmV4Vault>,
    pub system_program: Program<'info, System>,
}

pub fn fund(
    ctx: Context<FundAmmV4Vault>,
    kind: u8,
    amount: u64,
    expected_revision: u64,
) -> Result<()> {
    require!(amount > 0, CompanionError::ZeroAmount);
    let donation = ctx
        .accounts
        .donation_record
        .as_ref()
        .map(|record| (record.key(), &**record));
    let beneficiary = ctx
        .accounts
        .rights
        .beneficiary(kind, expected_revision, donation)?;
    require_keys_eq!(
        beneficiary,
        ctx.accounts.beneficiary.key(),
        CompanionError::InvalidRecipient
    );
    require_keys_neq!(
        ctx.accounts.payer.key(),
        ctx.accounts.vault.key(),
        CompanionError::AliasedAccounts
    );
    let vault = &ctx.accounts.vault;
    if vault.pool != Pubkey::default() {
        require!(
            vault.pool == ctx.accounts.rights.pool
                && vault.beneficiary == beneficiary
                && vault.kind == kind,
            CompanionError::InvalidVault
        );
    } else {
        require!(
            vault.credited == 0 && vault.claimed == 0,
            CompanionError::InvalidVault
        );
    }
    let credited = vault.next_credit(amount)?;
    let outstanding = vault.outstanding()?;
    let rent = Rent::get()?.minimum_balance(AmmV4Vault::SPACE);
    require!(
        ctx.accounts.vault.to_account_info().lamports()
            >= rent
                .checked_add(outstanding)
                .ok_or_else(|| error!(CompanionError::Overflow))?,
        CompanionError::Insolvent
    );
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        ),
        amount,
    )?;
    let vault = &mut ctx.accounts.vault;
    vault.pool = ctx.accounts.rights.pool;
    vault.beneficiary = beneficiary;
    vault.kind = kind;
    vault.credited = credited;
    emit!(CompanionDeposit {
        pool: vault.pool,
        beneficiary,
        kind,
        amount,
        revision: expected_revision,
        credited
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimAmmV4Vault<'info> {
    #[account(mut, seeds = [VAULT_SEED, vault.pool.as_ref(), &[vault.kind], vault.beneficiary.as_ref()], bump)]
    pub vault: Account<'info, AmmV4Vault>,
    /// CHECK: Fixed stored recipient; claims never accept an arbitrary destination.
    #[account(mut, address = vault.beneficiary)]
    pub beneficiary: UncheckedAccount<'info>,
}

pub fn claim(ctx: Context<ClaimAmmV4Vault>) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(AmmV4Vault::SPACE);
    let source = ctx.accounts.vault.to_account_info();
    let recipient = ctx.accounts.beneficiary.to_account_info();
    let amount = claim_balance(&mut ctx.accounts.vault, &source, &recipient, rent)?;
    emit!(CompanionClaim {
        pool: ctx.accounts.vault.pool,
        beneficiary: ctx.accounts.vault.beneficiary,
        kind: ctx.accounts.vault.kind,
        amount
    });
    Ok(())
}

/// Computes every fallible result before changing lamports or the ledger.
pub fn claim_balance(
    vault: &mut AmmV4Vault,
    source: &AccountInfo,
    recipient: &AccountInfo,
    rent: u64,
) -> Result<u64> {
    require_keys_eq!(*source.owner, crate::ID, CompanionError::InvalidVault);
    require_keys_eq!(
        *recipient.key,
        vault.beneficiary,
        CompanionError::InvalidRecipient
    );
    require_keys_neq!(*source.key, *recipient.key, CompanionError::AliasedAccounts);
    require!(
        source.is_writable && recipient.is_writable,
        CompanionError::InvalidVault
    );
    let amount = vault.outstanding()?;
    let remainder = source
        .lamports()
        .checked_sub(amount)
        .filter(|remaining| *remaining >= rent)
        .ok_or_else(|| error!(CompanionError::Insolvent))?;
    let destination = recipient
        .lamports()
        .checked_add(amount)
        .ok_or_else(|| error!(CompanionError::Overflow))?;
    let mut source_lamports = source.try_borrow_mut_lamports()?;
    let mut recipient_lamports = recipient.try_borrow_mut_lamports()?;
    **source_lamports = remainder;
    **recipient_lamports = destination;
    vault.claimed = vault.credited;
    Ok(amount)
}

#[event]
pub struct CompanionRightsChanged {
    pub pool: Pubkey,
    pub creator: Pubkey,
    pub revision: u64,
}
#[event]
pub struct CompanionDeposit {
    pub pool: Pubkey,
    pub beneficiary: Pubkey,
    pub kind: u8,
    pub amount: u64,
    pub revision: u64,
    pub credited: u64,
}
#[event]
pub struct CompanionClaim {
    pub pool: Pubkey,
    pub beneficiary: Pubkey,
    pub kind: u8,
    pub amount: u64,
}

#[error_code(offset = 7500)]
pub enum CompanionError {
    #[msg("Unauthorized companion-account action")]
    Unauthorized,
    #[msg("Unexpected pool-rights revision")]
    StaleRevision,
    #[msg("Invalid AMM v4 pool identity")]
    InvalidPool,
    #[msg("Expected a completed non-Mayhem native-SOL Beam curve")]
    InvalidCurve,
    #[msg("Invalid fixed beneficiary")]
    InvalidRecipient,
    #[msg("Donation recipient registration is revoked")]
    RecipientRevoked,
    #[msg("Invalid companion vault")]
    InvalidVault,
    #[msg("Companion deposits are paused")]
    Paused,
    #[msg("Invalid companion vault kind")]
    InvalidKind,
    #[msg("Amount must be positive")]
    ZeroAmount,
    #[msg("Companion accounting overflow")]
    Overflow,
    #[msg("Vault cannot cover credited balances and rent")]
    Insolvent,
    #[msg("Companion accounts must not alias")]
    AliasedAccounts,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{AccountDeserialize, AccountSerialize};

    fn key(n: u8) -> Pubkey {
        Pubkey::new_from_array([n; 32])
    }
    fn rights() -> AmmV4Rights {
        AmmV4Rights {
            pool: key(1),
            mint: key(2),
            market_seed: key(10),
            creator: key(3),
            creator_payout: key(3),
            donation_record: key(4),
            revision: 7,
            paused: false,
        }
    }
    fn ledger() -> AmmV4Vault {
        AmmV4Vault {
            pool: key(1),
            beneficiary: key(3),
            kind: CREATOR,
            credited: 500,
            claimed: 0,
        }
    }

    #[test]
    fn creator_succession_requires_all_roles_and_preserves_historical_credits() {
        let mut r = rights();
        let old = ledger();
        for (authority, creator, successor, revision) in [
            (key(8), key(3), key(5), 7),
            (key(9), key(8), key(5), 7),
            (key(9), key(3), key(5), 6),
            (key(9), key(3), Pubkey::default(), 7),
            (key(9), key(3), key(3), 7),
        ] {
            assert!(r
                .successor(authority, creator, successor, key(9), revision)
                .is_err());
            assert_eq!(r.creator, key(3));
            assert_eq!(r.revision, 7);
            assert_eq!(r.donation_record, key(4));
        }
        r.successor(key(9), key(3), key(5), key(9), 7).unwrap();
        assert_eq!(r.creator, key(5));
        assert_eq!(r.revision, 8);
        assert_eq!(r.donation_record, Pubkey::default());
        assert_eq!(old.beneficiary, key(3));
        assert_eq!(old.outstanding().unwrap(), 500);
        assert!(r.beneficiary(CREATOR, 7, None).is_err());
        assert_eq!(r.beneficiary(CREATOR, 8, None).unwrap(), key(5));
        assert!(r.successor(key(9), key(3), key(6), key(9), 8).is_err());
    }

    #[test]
    fn revoked_wrong_and_unselected_donation_records_cannot_receive_new_funds() {
        let mut r = rights();
        let record = DonationRecipient {
            recipient: key(6),
            active: true,
        };
        assert_eq!(
            r.beneficiary(DONATION, 7, Some((key(4), &record))).unwrap(),
            key(6)
        );
        assert!(r.beneficiary(DONATION, 6, Some((key(4), &record))).is_err());
        assert!(r.beneficiary(DONATION, 7, Some((key(5), &record))).is_err());
        assert!(r.beneficiary(DONATION, 7, None).is_err());
        let revoked = DonationRecipient {
            recipient: key(6),
            active: false,
        };
        assert!(r
            .beneficiary(DONATION, 7, Some((key(4), &revoked)))
            .is_err());
        assert!(r.beneficiary(2, 7, None).is_err());
        r.paused = true;
        assert!(r.beneficiary(CREATOR, 7, None).is_err());
        assert!(r.beneficiary(DONATION, 7, Some((key(4), &record))).is_err());
    }

    #[test]
    fn creator_and_donation_vaults_are_isolated_by_pool_kind_and_beneficiary() {
        let derive = |pool: Pubkey, kind: u8, beneficiary: Pubkey| {
            Pubkey::find_program_address(
                &[VAULT_SEED, pool.as_ref(), &[kind], beneficiary.as_ref()],
                &crate::ID,
            )
            .0
        };
        let canonical = derive(key(1), CREATOR, key(3));
        assert_ne!(canonical, derive(key(1), DONATION, key(3)));
        assert_ne!(canonical, derive(key(2), CREATOR, key(3)));
        assert_ne!(canonical, derive(key(1), CREATOR, key(4)));
        let mut unique = std::collections::HashSet::new();
        for pool in 1..=4 {
            for kind in [CREATOR, DONATION] {
                for beneficiary in 5..=9 {
                    assert!(unique.insert(derive(key(pool), kind, key(beneficiary))));
                }
            }
        }
    }

    #[test]
    fn claims_preserve_rent_and_unsolicited_surplus_and_cannot_be_replayed() {
        let pool = key(1);
        let beneficiary = key(3);
        let source_key = key(8);
        let owner = crate::ID;
        let recipient_owner = anchor_lang::system_program::ID;
        let mut source_lamports = 1_000; // 200 rent + 500 earned + 300 unsolicited.
        let mut recipient_lamports = 10;
        let mut source_data = [];
        let mut recipient_data = [];
        let source = AccountInfo::new(
            &source_key,
            false,
            true,
            &mut source_lamports,
            &mut source_data,
            &owner,
            false,
            0,
        );
        let recipient = AccountInfo::new(
            &beneficiary,
            false,
            true,
            &mut recipient_lamports,
            &mut recipient_data,
            &recipient_owner,
            false,
            0,
        );
        for kind in [CREATOR, DONATION] {
            let mut vault = AmmV4Vault {
                pool,
                beneficiary,
                kind,
                credited: 500,
                claimed: 0,
            };
            // A separate funded account would be used for each kind on-chain.
            **source.try_borrow_mut_lamports().unwrap() = 1_000;
            **recipient.try_borrow_mut_lamports().unwrap() = 10;
            assert_eq!(
                claim_balance(&mut vault, &source, &recipient, 200).unwrap(),
                500
            );
            assert_eq!(source.lamports(), 500);
            assert_eq!(recipient.lamports(), 510);
            assert_eq!(vault.outstanding().unwrap(), 0);
            assert_eq!(
                claim_balance(&mut vault, &source, &recipient, 200).unwrap(),
                0
            );
            assert_eq!(source.lamports(), 500);
            assert_eq!(recipient.lamports(), 510);
        }
    }

    #[test]
    fn invalid_claims_fail_without_changing_funds_or_credits() {
        for scenario in 0..8 {
            let mut vault = ledger();
            let source_key = key(8);
            let wrong_key = key(9);
            let beneficiary = vault.beneficiary;
            let owner = if scenario == 0 { key(7) } else { crate::ID };
            let destination_key = if scenario == 1 {
                wrong_key
            } else {
                beneficiary
            };
            let source_key = if scenario == 2 {
                destination_key
            } else {
                source_key
            };
            let mut source_amount = if scenario == 3 { 699 } else { 1_000 };
            let mut destination_amount = if scenario == 4 { u64::MAX } else { 10 };
            if scenario == 5 {
                vault.claimed = 501;
            }
            let before_source = source_amount;
            let before_destination = destination_amount;
            let before_claimed = vault.claimed;
            let mut source_data = [];
            let mut destination_data = [];
            let source = AccountInfo::new(
                &source_key,
                false,
                scenario != 6,
                &mut source_amount,
                &mut source_data,
                &owner,
                false,
                0,
            );
            let recipient = AccountInfo::new(
                &destination_key,
                false,
                scenario != 7,
                &mut destination_amount,
                &mut destination_data,
                &owner,
                false,
                0,
            );
            assert!(
                claim_balance(&mut vault, &source, &recipient, 200).is_err(),
                "scenario {scenario}"
            );
            assert_eq!(source.lamports(), before_source);
            assert_eq!(recipient.lamports(), before_destination);
            assert_eq!(vault.claimed, before_claimed);
        }
    }

    #[test]
    fn credit_and_revision_overflows_are_atomic() {
        let mut vault = ledger();
        vault.credited = u64::MAX;
        assert!(vault.next_credit(1).is_err());
        assert_eq!(vault.credited, u64::MAX);
        let mut r = rights();
        r.revision = u64::MAX;
        assert!(r
            .successor(key(9), key(3), key(5), key(9), u64::MAX)
            .is_err());
        assert_eq!(r.creator, key(3));
        assert_eq!(r.donation_record, key(4));
        assert_eq!(r.revision, u64::MAX);
    }

    #[test]
    fn companion_account_spaces_match_serialized_layouts() {
        let r = rights();
        let v = ledger();
        let d = DonationRecipient {
            recipient: key(6),
            active: true,
        };
        let mut encoded = Vec::new();
        r.try_serialize(&mut encoded).unwrap();
        assert_eq!(encoded.len(), AmmV4Rights::SPACE);
        assert_eq!(
            AmmV4Rights::try_deserialize(&mut encoded.as_slice())
                .unwrap()
                .creator,
            r.creator
        );
        encoded.clear();
        v.try_serialize(&mut encoded).unwrap();
        assert_eq!(encoded.len(), AmmV4Vault::SPACE);
        encoded.clear();
        d.try_serialize(&mut encoded).unwrap();
        assert_eq!(encoded.len(), DonationRecipient::SPACE);
    }

    #[test]
    fn registration_rejects_foreign_owners_wrong_pool_mints_and_layouts() {
        let mint = key(2);
        let market = key(10);
        let a = beam_amm_v4::derive_pool_addresses(&AMM_PROGRAM, &market).unwrap();
        let mut data = vec![0u8; 752];
        data[..8].copy_from_slice(&6u64.to_le_bytes());
        data[8..16].copy_from_slice(&u64::from(a.nonce).to_le_bytes());
        for (offset, value) in [
            (336, a.coin_vault),
            (368, a.pc_vault),
            (400, mint),
            (432, anchor_spl::token::spl_token::native_mint::ID),
            (464, a.lp_mint),
            (528, market),
            (592, a.target_orders),
        ] {
            data[offset..offset + 32].copy_from_slice(value.as_ref());
        }
        validate_pool_identity(a.amm, AMM_PROGRAM, &data, mint, market).unwrap();
        assert!(validate_pool_identity(a.amm, key(11), &data, mint, market).is_err());
        assert!(validate_pool_identity(key(11), AMM_PROGRAM, &data, mint, market).is_err());
        assert!(validate_pool_identity(a.amm, AMM_PROGRAM, &data, key(11), market).is_err());
        assert!(validate_pool_identity(a.amm, AMM_PROGRAM, &data, mint, key(11)).is_err());
        for len in [0, 8, 751, 753] {
            let mut bytes = data.clone();
            bytes.resize(len, 0);
            assert!(validate_pool_identity(a.amm, AMM_PROGRAM, &bytes, mint, market).is_err());
        }
        for offset in [0usize, 8, 336, 368, 400, 432, 464, 528, 592] {
            let mut bytes = data.clone();
            bytes[offset] ^= 0xff;
            assert!(
                validate_pool_identity(a.amm, AMM_PROGRAM, &bytes, mint, market).is_err(),
                "offset {offset}"
            );
        }
    }
}
