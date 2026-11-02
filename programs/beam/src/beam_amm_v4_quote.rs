//! Graduated native-SOL quote-chain adapter for a jointly registered AMM v4 pool.
//! Spot reserves are not a TWAP/oracle. Only swap-only, orderbook-free pools
//! are accepted. The immutable PumpSwap pool decoder is never used.
use crate::{
    beam_amm_v4,
    beam_amm_v4_companion::{validate_pool_identity, AmmV4Rights, AMM_PROGRAM, RIGHTS_SEED},
    pump_quote::Record,
    trade_types::BondingCurve,
};
use anchor_lang::{
    prelude::*,
    solana_program::{program_option::COption, program_pack::Pack},
};
use anchor_spl::token::spl_token::{
    self,
    state::{Account as SplAccount, AccountState},
};

fn token(info: &AccountInfo, mint: Pubkey, authority: Pubkey, native: bool) -> Result<u64> {
    require_keys_eq!(*info.owner, spl_token::ID, QuoteError::InvalidVault);
    let account = SplAccount::unpack(&info.try_borrow_data()?)
        .map_err(|_| error!(QuoteError::InvalidVault))?;
    require!(
        account.mint == mint
            && account.owner == authority
            && account.state == AccountState::Initialized
            && account.delegate == COption::None
            && account.close_authority == COption::None,
        QuoteError::InvalidVault
    );
    match (native, account.is_native) {
        (false, COption::None) => (),
        (true, COption::Some(rent)) => require!(
            rent.checked_add(account.amount)
                .map_or(false, |backing| info.lamports() >= backing),
            QuoteError::InvalidVault
        ),
        _ => return err!(QuoteError::InvalidVault),
    }
    Ok(account.amount)
}

/// Tail layout: curve (already authenticated by the walker), rights, pool,
/// coin vault, PC/WSOL vault. Active ancestors retain their one-account layout.
pub fn graduated_record(
    mint: &Pubkey,
    curve: &BondingCurve,
    tail: &mut &[AccountInfo],
) -> Result<Record> {
    require!(tail.len() >= 5, QuoteError::MissingAccounts);
    require!(
        curve.complete
            && !curve.is_mayhem_mode
            && (curve.quote_mint == Pubkey::default()
                || curve.quote_mint == spl_token::native_mint::ID),
        QuoteError::InvalidCurve
    );
    let right_info = &tail[1];
    require_keys_eq!(*right_info.owner, crate::ID, QuoteError::InvalidRights);
    let rights = AmmV4Rights::try_deserialize(&mut &right_info.try_borrow_data()?[..])?;
    let expected = Pubkey::find_program_address(&[RIGHTS_SEED, rights.pool.as_ref()], &crate::ID).0;
    require_keys_eq!(right_info.key(), expected, QuoteError::InvalidRights);
    require!(
        rights.mint == *mint && !rights.paused,
        QuoteError::InvalidRights
    );
    let pool = &tail[2];
    require_keys_eq!(pool.key(), rights.pool, QuoteError::InvalidPool);
    let data = pool.try_borrow_data()?;
    validate_pool_identity(pool.key(), *pool.owner, &data, *mint, rights.market_seed)?;
    // Status 6 is SwapOnly in the pinned source. Do not omit an active
    // orderbook position when valuing a legacy initialized/status-1 pool.
    require!(
        u64::from_le_bytes(data[..8].try_into().unwrap()) == 6,
        QuoteError::InvalidPool
    );
    let addresses = beam_amm_v4::derive_pool_addresses(&AMM_PROGRAM, &rights.market_seed)
        .map_err(|_| error!(QuoteError::InvalidPool))?;
    require_keys_eq!(
        tail[3].key(),
        addresses.coin_vault,
        QuoteError::InvalidVault
    );
    require_keys_eq!(tail[4].key(), addresses.pc_vault, QuoteError::InvalidVault);
    let base = token(&tail[3], *mint, addresses.authority, false)?
        .checked_sub(u64::from_le_bytes(data[192..200].try_into().unwrap()))
        .ok_or_else(|| error!(QuoteError::InvalidBalance))?;
    let quote = token(
        &tail[4],
        spl_token::native_mint::ID,
        addresses.authority,
        true,
    )?
    .checked_sub(u64::from_le_bytes(data[200..208].try_into().unwrap()))
    .ok_or_else(|| error!(QuoteError::InvalidBalance))?;
    require!(base > 0 && quote > 0, QuoteError::InvalidBalance);
    drop(data);
    *tail = &tail[5..];
    Ok(Record::Pool {
        base,
        quote: u128::from(quote),
    })
}

#[error_code(offset = 7700)]
pub enum QuoteError {
    #[msg("Graduated quote requires curve, registered rights, AMM v4 pool and both vaults")]
    MissingAccounts,
    #[msg("Invalid completed native-SOL ancestor")]
    InvalidCurve,
    #[msg("Invalid registered AMM v4 rights")]
    InvalidRights,
    #[msg("Invalid swap-only AMM v4 quote pool")]
    InvalidPool,
    #[msg("Invalid AMM v4 quote vault")]
    InvalidVault,
    #[msg("AMM v4 reserves cannot cover pending PnL or are empty")]
    InvalidBalance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{AccountSerialize, Discriminator};
    struct Fixture {
        keys: [Pubkey; 5],
        owners: [Pubkey; 5],
        balances: [u64; 5],
        data: [Vec<u8>; 5],
        mint: Pubkey,
    }
    impl Fixture {
        fn new() -> Self {
            let mint = Pubkey::new_unique();
            let market_seed = Pubkey::new_unique();
            let a = beam_amm_v4::derive_pool_addresses(&AMM_PROGRAM, &market_seed).unwrap();
            let rights = AmmV4Rights {
                pool: a.amm,
                mint,
                market_seed,
                creator: Pubkey::new_unique(),
                creator_payout: Pubkey::new_unique(),
                donation_record: Pubkey::default(),
                revision: 0,
                paused: false,
            };
            let right_key =
                Pubkey::find_program_address(&[RIGHTS_SEED, a.amm.as_ref()], &crate::ID).0;
            let mut encoded = Vec::new();
            rights.try_serialize(&mut encoded).unwrap();
            let mut curve = vec![0; 166];
            curve[..8].copy_from_slice(BondingCurve::DISCRIMINATOR);
            curve[48] = 1;
            let mut pool = vec![0; 752];
            pool[..8].copy_from_slice(&6u64.to_le_bytes());
            pool[8..16].copy_from_slice(&u64::from(a.nonce).to_le_bytes());
            pool[192..200].copy_from_slice(&100u64.to_le_bytes());
            pool[200..208].copy_from_slice(&200u64.to_le_bytes());
            for (offset, key) in [
                (336, a.coin_vault),
                (368, a.pc_vault),
                (400, mint),
                (432, spl_token::native_mint::ID),
                (464, a.lp_mint),
                (528, market_seed),
                (592, a.target_orders),
            ] {
                pool[offset..offset + 32].copy_from_slice(key.as_ref());
            }
            let token = |mint, amount, native| {
                let mut data = vec![0; SplAccount::LEN];
                SplAccount::pack(
                    SplAccount {
                        mint,
                        owner: a.authority,
                        amount,
                        delegate: COption::None,
                        state: AccountState::Initialized,
                        is_native: if native {
                            COption::Some(10)
                        } else {
                            COption::None
                        },
                        delegated_amount: 0,
                        close_authority: COption::None,
                    },
                    &mut data,
                )
                .unwrap();
                data
            };
            Self {
                keys: [
                    Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], &crate::ID).0,
                    right_key,
                    a.amm,
                    a.coin_vault,
                    a.pc_vault,
                ],
                owners: [
                    crate::ID,
                    crate::ID,
                    AMM_PROGRAM,
                    spl_token::ID,
                    spl_token::ID,
                ],
                balances: [100_000_000; 5],
                data: [
                    curve,
                    encoded,
                    pool,
                    token(mint, 9_000_000, false),
                    token(spl_token::native_mint::ID, 18_000_000, true),
                ],
                mint,
            }
        }
        fn read(&mut self, count: usize) -> Result<Record> {
            let mint = self.mint;
            let curve = BondingCurve::try_deserialize(&mut self.data[0].as_slice()).unwrap();
            let infos: Vec<_> = self
                .keys
                .iter()
                .zip(self.owners.iter())
                .zip(self.balances.iter_mut())
                .zip(self.data.iter_mut())
                .map(|(((key, owner), balance), data)| {
                    AccountInfo::new(key, false, false, balance, data, owner, false, 0)
                })
                .collect();
            let mut tail = &infos[..count];
            let result = graduated_record(&mint, &curve, &mut tail);
            assert_eq!(tail.len(), if result.is_ok() { count - 5 } else { count });
            result
        }
    }
    #[test]
    fn graduated_quote_uses_registered_native_vault_reserves_after_pending_pnl() {
        let mut f = Fixture::new();
        match f.read(5).unwrap() {
            Record::Pool { base, quote } => {
                assert_eq!(base, 8_999_900);
                assert_eq!(quote, 17_999_800);
            }
            _ => panic!("expected AMM v4 pool reserves"),
        }
    }
    #[test]
    fn graduated_quote_rejects_substitutions_missing_accounts_and_unsafe_reserves() {
        for count in 0..5 {
            assert!(Fixture::new().read(count).is_err());
        }
        for scenario in 0..13 {
            let mut f = Fixture::new();
            match scenario {
                0 => f.owners[1] = Pubkey::new_unique(),
                1 => f.keys[1] = Pubkey::new_unique(),
                2 => f.owners[2] = Pubkey::new_unique(),
                3 => f.keys[2] = Pubkey::new_unique(),
                4 => f.keys[3] = Pubkey::new_unique(),
                5 => f.keys[4] = Pubkey::new_unique(),
                6 => f.owners[3] = Pubkey::new_unique(),
                7 => f.data[2][..8].copy_from_slice(&1u64.to_le_bytes()),
                8 => f.data[2][192..200].copy_from_slice(&u64::MAX.to_le_bytes()),
                9 => f.data[2][200..208].copy_from_slice(&u64::MAX.to_le_bytes()),
                10 => f.balances[4] = 1,
                11 => f.data[0][81] = 1,                     // Mayhem flag.
                12 => f.data[1][AmmV4Rights::SPACE - 1] = 1, // Paused.
                _ => unreachable!(),
            }
            assert!(f.read(5).is_err(), "scenario {scenario}");
        }
    }
}
