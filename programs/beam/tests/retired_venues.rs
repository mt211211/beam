//! Native Anchor dispatch tests. These are not a validator/SBF run.
use anchor_lang::solana_program::{
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    program_stubs::{set_syscall_stubs, SyscallStubs},
};
use anchor_lang::{prelude::*, InstructionData, ToAccountMetas};
use beam_program as beam;
use pump_reference as reference;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct NoCpi(Arc<AtomicUsize>);
impl SyscallStubs for NoCpi {
    fn sol_invoke_signed(
        &self,
        _ix: &Instruction,
        _infos: &[AccountInfo],
        _seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ProgramError::IncorrectProgramId)
    }
}
struct RestoreStubs(Option<Box<dyn SyscallStubs>>);
impl Drop for RestoreStubs {
    fn drop(&mut self) {
        set_syscall_stubs(self.0.take().unwrap());
    }
}

fn dispatch(metas: Vec<AccountMeta>, data: Vec<u8>, expected: u32) {
    let mut balances = vec![1_000_000u64; metas.len()];
    let mut contents = vec![vec![0x55u8; 8]; metas.len()];
    let owners = vec![Pubkey::new_unique(); metas.len()];
    let infos: Vec<_> = metas
        .iter()
        .zip(balances.iter_mut())
        .zip(contents.iter_mut())
        .zip(owners.iter())
        .map(|(((meta, lamports), data), owner)| {
            AccountInfo::new(
                &meta.pubkey,
                meta.is_signer,
                meta.is_writable,
                lamports,
                data,
                owner,
                false,
                0,
            )
        })
        .collect();
    assert_eq!(
        beam::entry(&beam::ID, &infos, &data),
        Err(ProgramError::Custom(expected))
    );
    for info in &infos {
        assert_eq!(info.lamports(), 1_000_000);
        assert_eq!(&**info.try_borrow_data().unwrap(), &[0x55u8; 8]);
    }
}

#[test]
fn retired_interfaces_preserve_order_and_dispatch_without_foreign_accounts_or_cpis() {
    let calls = Arc::new(AtomicUsize::new(0));
    let _restore = RestoreStubs(Some(set_syscall_stubs(Box::new(NoCpi(calls.clone())))));
    {
        let keys: Vec<_> = (0..25).map(|_| Pubkey::new_unique()).collect();
        let metas = beam::accounts::Migrate {
            global: keys[0],
            withdraw_authority: keys[1],
            mint: keys[2],
            bonding_curve: keys[3],
            associated_bonding_curve: keys[4],
            user: keys[5],
            system_program: keys[6],
            token_program: keys[7],
            pump_amm: keys[8],
            pool: keys[9],
            pool_authority: keys[10],
            pool_authority_mint_account: keys[11],
            pool_authority_wsol_account: keys[12],
            amm_global_config: keys[13],
            wsol_mint: keys[14],
            lp_mint: keys[15],
            user_pool_token_account: keys[16],
            pool_base_token_account: keys[17],
            pool_quote_token_account: keys[18],
            token_2022_program: keys[19],
            associated_token_program: keys[20],
            pump_amm_event_authority: keys[21],
            event_authority: keys[22],
            program: keys[23],
            rent: keys[24],
        }
        .to_account_metas(None);
        let mut original = reference::accounts::Migrate {
            global: keys[0],
            withdraw_authority: keys[1],
            mint: keys[2],
            bonding_curve: keys[3],
            associated_bonding_curve: keys[4],
            user: keys[5],
            system_program: keys[6],
            token_program: keys[7],
            pump_amm: keys[8],
            pool: keys[9],
            pool_authority: keys[10],
            pool_authority_mint_account: keys[11],
            pool_authority_wsol_account: keys[12],
            amm_global_config: keys[13],
            wsol_mint: keys[14],
            lp_mint: keys[15],
            user_pool_token_account: keys[16],
            pool_base_token_account: keys[17],
            pool_quote_token_account: keys[18],
            token_2022_program: keys[19],
            associated_token_program: keys[20],
            pump_amm_event_authority: keys[21],
            event_authority: keys[22],
            program: keys[23],
            rent: keys[24],
        }
        .to_account_metas(None);
        assert_eq!(metas, original, "account order/privileges changed");
        let data = beam::instruction::Migrate {}.data();
        assert_eq!(data, reference::instruction::Migrate {}.data());
        dispatch(metas, data, 7001);
    }
    {
        let keys: Vec<_> = (0..27).map(|_| Pubkey::new_unique()).collect();
        let metas = beam::accounts::MigrateV2 {
            global: keys[0],
            withdraw_authority: keys[1],
            base_mint: keys[2],
            quote_mint: keys[3],
            bonding_curve: keys[4],
            associated_base_bonding_curve: keys[5],
            associated_quote_bonding_curve: keys[6],
            user: keys[7],
            system_program: keys[8],
            pump_amm: keys[9],
            pool: keys[10],
            pool_authority: keys[11],
            pool_authority_mint_account: keys[12],
            pool_authority_quote_account: keys[13],
            amm_global_config: keys[14],
            lp_mint: keys[15],
            user_pool_token_account: keys[16],
            pool_base_token_account: keys[17],
            pool_quote_token_account: keys[18],
            base_token_program: keys[19],
            quote_token_program: keys[20],
            token_2022_program: keys[21],
            associated_token_program: keys[22],
            pump_amm_event_authority: keys[23],
            rent: keys[24],
            event_authority: keys[25],
            program: keys[26],
        }
        .to_account_metas(None);
        let mut original = reference::accounts::MigrateV2 {
            global: keys[0],
            withdraw_authority: keys[1],
            base_mint: keys[2],
            quote_mint: keys[3],
            bonding_curve: keys[4],
            associated_base_bonding_curve: keys[5],
            associated_quote_bonding_curve: keys[6],
            user: keys[7],
            system_program: keys[8],
            pump_amm: keys[9],
            pool: keys[10],
            pool_authority: keys[11],
            pool_authority_mint_account: keys[12],
            pool_authority_quote_account: keys[13],
            amm_global_config: keys[14],
            lp_mint: keys[15],
            user_pool_token_account: keys[16],
            pool_base_token_account: keys[17],
            pool_quote_token_account: keys[18],
            base_token_program: keys[19],
            quote_token_program: keys[20],
            token_2022_program: keys[21],
            associated_token_program: keys[22],
            pump_amm_event_authority: keys[23],
            rent: keys[24],
            event_authority: keys[25],
            program: keys[26],
        }
        .to_account_metas(None);
        assert_eq!(metas, original, "account order/privileges changed");
        let data = beam::instruction::MigrateV2 {}.data();
        assert_eq!(data, reference::instruction::MigrateV2 {}.data());
        dispatch(metas, data, 7001);
    }
    {
        let keys: Vec<_> = (0..13).map(|_| Pubkey::new_unique()).collect();
        let metas = beam::accounts::MultiHopCurve {
            global: keys[0],
            fee_config: keys[1],
            pump_amm_global_config: keys[2],
            user: keys[3],
            source_authority: keys[4],
            source: keys[5],
            destination: keys[6],
            buyback_fee_recipient: keys[7],
            token_program: keys[8],
            token_2022_program: keys[9],
            system_program: keys[10],
            event_authority: keys[11],
            program: keys[12],
        }
        .to_account_metas(None);
        let mut original = reference::accounts::MultiHopCurve {
            global: keys[0],
            fee_config: keys[1],
            pump_amm_global_config: keys[2],
            user: keys[3],
            source_authority: keys[4],
            source: keys[5],
            destination: keys[6],
            buyback_fee_recipient: keys[7],
            token_program: keys[8],
            token_2022_program: keys[9],
            system_program: keys[10],
            event_authority: keys[11],
            program: keys[12],
        }
        .to_account_metas(None);
        original[2].is_signer = false; // Retired foreign PDA no longer signs.
        assert_eq!(metas, original, "account order/privileges changed");
        let data = beam::instruction::MultiHopCurveSwap {
            amount_in: 5,
            hop_fees: vec![0],
        }
        .data();
        assert_eq!(
            data,
            reference::instruction::MultiHopCurveSwap {
                amount_in: 5,
                hop_fees: vec![0]
            }
            .data()
        );
        dispatch(metas, data, 7001);
    }
    {
        let keys: Vec<_> = (0..26).map(|_| Pubkey::new_unique()).collect();
        let metas = beam::accounts::AdminCto {
            admin_set_creator_authority: keys[0],
            global: keys[1],
            mint: keys[2],
            quote_mint: keys[3],
            quote_token_program: keys[4],
            associated_token_program: keys[5],
            system_program: keys[6],
            bonding_curve: keys[7],
            current_creator: keys[8],
            current_creator_quote_token_account: keys[9],
            creator_vault: keys[10],
            creator_vault_quote_token_account: keys[11],
            holder_creator_vault: keys[12],
            holder_creator_vault_quote_token_account: keys[13],
            pump_amm: keys[14],
            amm_global_config: keys[15],
            pool_authority: keys[16],
            pool: keys[17],
            pump_amm_event_authority: keys[18],
            coin_creator_vault_authority: keys[19],
            coin_creator_vault_ata: keys[20],
            sharing_config: keys[21],
            pump_fees: keys[22],
            pump_fees_event_authority: keys[23],
            event_authority: keys[24],
            program: keys[25],
        }
        .to_account_metas(None);
        let mut original = reference::accounts::AdminCto {
            admin_set_creator_authority: keys[0],
            global: keys[1],
            mint: keys[2],
            quote_mint: keys[3],
            quote_token_program: keys[4],
            associated_token_program: keys[5],
            system_program: keys[6],
            bonding_curve: keys[7],
            current_creator: keys[8],
            current_creator_quote_token_account: keys[9],
            creator_vault: keys[10],
            creator_vault_quote_token_account: keys[11],
            holder_creator_vault: keys[12],
            holder_creator_vault_quote_token_account: keys[13],
            pump_amm: keys[14],
            amm_global_config: keys[15],
            pool_authority: keys[16],
            pool: keys[17],
            pump_amm_event_authority: keys[18],
            coin_creator_vault_authority: keys[19],
            coin_creator_vault_ata: keys[20],
            sharing_config: keys[21],
            pump_fees: keys[22],
            pump_fees_event_authority: keys[23],
            event_authority: keys[24],
            program: keys[25],
        }
        .to_account_metas(None);
        assert_eq!(metas, original, "account order/privileges changed");
        let data = beam::instruction::AdminCto {
            is_holder_reward: None,
            creator_fee_bps: None,
            new_creator: None,
        }
        .data();
        assert_eq!(
            data,
            reference::instruction::AdminCto {
                is_holder_reward: None,
                creator_fee_bps: None,
                new_creator: None
            }
            .data()
        );
        dispatch(metas, data, 7001);
    }
    {
        let keys: Vec<_> = (0..8).map(|_| Pubkey::new_unique()).collect();
        let metas = beam::accounts::SetMayhemVirtualParams {
            sol_vault_authority: keys[0],
            mayhem_token_vault: keys[1],
            mint: keys[2],
            global: keys[3],
            bonding_curve: keys[4],
            token_program: keys[5],
            event_authority: keys[6],
            program: keys[7],
        }
        .to_account_metas(None);
        let mut original = reference::accounts::SetMayhemVirtualParams {
            sol_vault_authority: keys[0],
            mayhem_token_vault: keys[1],
            mint: keys[2],
            global: keys[3],
            bonding_curve: keys[4],
            token_program: keys[5],
            event_authority: keys[6],
            program: keys[7],
        }
        .to_account_metas(None);
        assert_eq!(metas, original, "account order/privileges changed");
        let data = beam::instruction::SetMayhemVirtualParams {}.data();
        assert_eq!(
            data,
            reference::instruction::SetMayhemVirtualParams {}.data()
        );
        dispatch(metas, data, 7000);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "retired handler attempted a CPI"
    );
}
