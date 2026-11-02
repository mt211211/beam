//! Synthetic local-bank execution of the actual Anchor companion instructions.
//! The Raydium pool is a source-layout fixture. No Raydium processor, swap,
//! live network, wallet or production funds are used.
use anchor_lang::solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_option::COption,
    program_pack::Pack,
};
use anchor_lang::{
    AccountDeserialize, AccountSerialize, Discriminator, InstructionData, ToAccountMetas,
};
use anchor_spl::token::spl_token;
use beam_program::{
    self as beam, beam_amm_v4_companion::*, beam_amm_v4_router::*, beam_amm_v4_sharing::*,
    trade_types::BondingCurve, Global,
};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::Account,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction, system_program,
    transaction::Transaction,
};

fn processor_entry<'a>(id: &Pubkey, accounts: &[AccountInfo<'a>], bytes: &[u8]) -> ProgramResult {
    // SAFETY: ProgramTest dispatch is synchronous and neither side retains the slice.
    let accounts: &'a [AccountInfo<'a>] = unsafe { std::mem::transmute(accounts) };
    beam::entry(id, accounts, bytes)
}

/// Synthetic constant-price processor. Tests Beam's CPI wiring and rollback;
/// it is not a Raydium execution or pricing-parity claim.
fn mock_amm(id: &Pubkey, infos: &[AccountInfo], bytes: &[u8]) -> ProgramResult {
    if infos.len() != 8 || bytes.len() != 17 || bytes[0] != 16 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let amount = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
    let minimum = u64::from_le_bytes(bytes[9..17].try_into().unwrap());
    let source = spl_token::state::Account::unpack(&infos[5].try_borrow_data()?)?;
    let buy = source.mint == spl_token::native_mint::ID;
    let (input_vault, output_vault) = if buy {
        (&infos[4], &infos[3])
    } else {
        (&infos[3], &infos[4])
    };
    let input = spl_token::instruction::transfer(
        &spl_token::ID,
        infos[5].key,
        input_vault.key,
        infos[7].key,
        &[],
        amount,
    )?;
    invoke(
        &input,
        &[
            infos[5].clone(),
            input_vault.clone(),
            infos[7].clone(),
            infos[0].clone(),
        ],
    )?;
    let output = if buy {
        amount.checked_mul(2).unwrap()
    } else {
        amount / 2
    };
    // Intentionally fail after the input CPI to exercise transaction rollback.
    if output < minimum {
        return Err(ProgramError::Custom(123));
    }
    let (_, nonce) = Pubkey::find_program_address(&[b"amm authority"], id);
    let transfer = spl_token::instruction::transfer(
        &spl_token::ID,
        output_vault.key,
        infos[6].key,
        infos[2].key,
        &[],
        output,
    )?;
    invoke_signed(
        &transfer,
        &[
            output_vault.clone(),
            infos[6].clone(),
            infos[2].clone(),
            infos[0].clone(),
        ],
        &[&[b"amm authority", &[nonce]]],
    )
}
fn token_account(mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    let reserve = 2_039_280;
    let native = mint == spl_token::native_mint::ID;
    let mut data = vec![0; spl_token::state::Account::LEN];
    spl_token::state::Account::pack(
        spl_token::state::Account {
            mint,
            owner,
            amount,
            delegate: COption::None,
            state: spl_token::state::AccountState::Initialized,
            is_native: if native {
                COption::Some(reserve)
            } else {
                COption::None
            },
            delegated_amount: 0,
            close_authority: COption::None,
        },
        &mut data,
    )
    .unwrap();
    Account {
        lamports: reserve + if native { amount } else { 0 },
        data,
        owner: spl_token::ID,
        executable: false,
        rent_epoch: 0,
    }
}
async fn tokens(ctx: &mut ProgramTestContext, address: Pubkey) -> u64 {
    spl_token::state::Account::unpack(&stored(ctx, address).await.data)
        .unwrap()
        .amount
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &beam::ID).0
}
fn account(owner: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: 100_000_000,
        data,
        owner,
        executable: false,
        rent_epoch: 0,
    }
}
fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: beam::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}
async fn send(
    ctx: &mut ProgramTestContext,
    ix: Instruction,
    signers: &[&Keypair],
    nonce: u32,
) -> bool {
    let mut all = vec![&ctx.payer];
    all.extend_from_slice(signers);
    let tx = Transaction::new_signed_with_payer(
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(300_000 + nonce),
            ix,
        ],
        Some(&ctx.payer.pubkey()),
        &all,
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.is_ok()
}
async fn stored(ctx: &mut ProgramTestContext, address: Pubkey) -> Account {
    ctx.banks_client
        .get_account(address)
        .await
        .unwrap()
        .unwrap()
}
async fn balance(ctx: &mut ProgramTestContext, address: Pubkey) -> u64 {
    ctx.banks_client.get_balance(address).await.unwrap()
}

#[tokio::test]
async fn creator_and_donation_vaults_execute_with_revision_checks_and_conserved_funds() {
    let admin = Keypair::new();
    let cto_admin = Keypair::new();
    let creator = Keypair::new();
    let successor = Keypair::new();
    let recipient = Keypair::new();
    let attacker = Keypair::new();
    let trader = Keypair::new();
    let user_quote = Pubkey::new_unique();
    let user_coin = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let market = Pubkey::new_unique();
    let addresses = beam::beam_amm_v4::derive_pool_addresses(&AMM_PROGRAM, &market).unwrap();
    let global = pda(&[b"global"]);
    let curve = pda(&[b"bonding-curve", mint.as_ref()]);
    let rights = pda(&[RIGHTS_SEED, addresses.amm.as_ref()]);
    let record = pda(&[DONATION_SEED, recipient.pubkey().as_ref()]);
    let old_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[CREATOR],
        creator.pubkey().as_ref(),
    ]);
    let new_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[CREATOR],
        successor.pubkey().as_ref(),
    ]);
    let donation_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[DONATION],
        recipient.pubkey().as_ref(),
    ]);

    let mut global_bytes = vec![0u8; 2048];
    global_bytes[..8].copy_from_slice(Global::DISCRIMINATOR);
    global_bytes[8] = 1;
    global_bytes[9..41].copy_from_slice(admin.pubkey().as_ref());
    let mut global_state = Global::try_deserialize(&mut global_bytes.as_slice()).unwrap();
    global_state.admin_set_creator_authority = cto_admin.pubkey();
    global_bytes.clear();
    global_state.try_serialize(&mut global_bytes).unwrap();
    let mut curve_bytes = vec![0u8; 166];
    curve_bytes[..8].copy_from_slice(BondingCurve::DISCRIMINATOR);
    curve_bytes[48] = 1; // complete
    curve_bytes[49..81].copy_from_slice(creator.pubkey().as_ref());
    let state = BondingCurve::try_deserialize(&mut curve_bytes.as_slice()).unwrap();
    assert!(state.complete && !state.is_mayhem_mode);
    assert_eq!(state.creator, creator.pubkey());

    let mut mint_bytes = vec![0u8; spl_token::state::Mint::LEN];
    spl_token::state::Mint::pack(
        spl_token::state::Mint {
            mint_authority: COption::None,
            supply: 1_000_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        &mut mint_bytes,
    )
    .unwrap();
    let mut pool_bytes = vec![0u8; 752];
    pool_bytes[..8].copy_from_slice(&6u64.to_le_bytes());
    pool_bytes[8..16].copy_from_slice(&u64::from(addresses.nonce).to_le_bytes());
    for (offset, key) in [
        (336, addresses.coin_vault),
        (368, addresses.pc_vault),
        (400, mint),
        (432, spl_token::native_mint::ID),
        (464, addresses.lp_mint),
        (528, market),
        (592, addresses.target_orders),
    ] {
        pool_bytes[offset..offset + 32].copy_from_slice(key.as_ref());
    }

    let mut bank = ProgramTest::new("beam_program", beam::ID, processor!(processor_entry));
    bank.add_program("mock_amm", AMM_PROGRAM, processor!(mock_amm));
    bank.add_program(
        "spl_token",
        spl_token::ID,
        processor!(spl_token::processor::Processor::process),
    );
    bank.add_account(
        addresses.coin_vault,
        token_account(mint, addresses.authority, 1_000_000_000),
    );
    bank.add_account(
        addresses.pc_vault,
        token_account(
            spl_token::native_mint::ID,
            addresses.authority,
            1_000_000_000,
        ),
    );
    bank.add_account(
        user_quote,
        token_account(spl_token::native_mint::ID, trader.pubkey(), 100_000_000),
    );
    bank.add_account(user_coin, token_account(mint, trader.pubkey(), 0));
    bank.add_account(addresses.authority, account(system_program::ID, vec![]));
    bank.add_account(global, account(beam::ID, global_bytes));
    bank.add_account(curve, account(beam::ID, curve_bytes));
    bank.add_account(mint, account(spl_token::ID, mint_bytes));
    bank.add_account(addresses.amm, account(AMM_PROGRAM, pool_bytes.clone()));
    for signer in [
        &admin, &creator, &successor, &recipient, &attacker, &trader, &cto_admin,
    ] {
        bank.add_account(signer.pubkey(), account(system_program::ID, vec![]));
    }
    let mut ctx = bank.start_with_context().await;
    let register = |authority, who| {
        instruction(
            beam::accounts::RegisterAmmV4Rights {
                global,
                authority,
                creator: who,
                mint,
                bonding_curve: curve,
                pool: addresses.amm,
                rights,
                system_program: system_program::ID,
            },
            beam::instruction::RegisterAmmV4Rights {
                market_seed: market,
            },
        )
    };
    assert!(
        !send(
            &mut ctx,
            register(attacker.pubkey(), creator.pubkey()),
            &[&attacker, &creator],
            1
        )
        .await
    );
    assert!(ctx
        .banks_client
        .get_account(rights)
        .await
        .unwrap()
        .is_none());
    assert!(
        send(
            &mut ctx,
            register(admin.pubkey(), creator.pubkey()),
            &[&admin, &creator],
            2
        )
        .await
    );

    let fund = |payer, who, vault, kind, amount, revision, donation_record| {
        instruction(
            beam::accounts::FundAmmV4Vault {
                payer,
                rights,
                beneficiary: who,
                donation_record,
                vault,
                system_program: system_program::ID,
            },
            beam::instruction::FundAmmV4Vault {
                kind,
                amount,
                revision,
            },
        )
    };
    let payer = ctx.payer.pubkey();
    assert!(
        send(
            &mut ctx,
            fund(
                payer,
                creator.pubkey(),
                old_vault,
                CREATOR,
                5_000_000,
                0,
                None
            ),
            &[],
            3
        )
        .await
    );
    let before = stored(&mut ctx, old_vault).await;
    let ledger = AmmV4Vault::try_deserialize(&mut before.data.as_slice()).unwrap();
    assert_eq!(ledger.credited, 5_000_000);
    assert_eq!(ledger.claimed, 0);
    let vault_rent = before.lamports - 5_000_000;

    // An unsolicited transfer is not an earned credit.
    assert!(
        send(
            &mut ctx,
            system_instruction::transfer(&payer, &old_vault, 1_000_000),
            &[],
            4
        )
        .await
    );
    let cto = |authority, old, new, revision| {
        instruction(
            beam::accounts::ChangeAmmV4Creator {
                global,
                authority,
                creator: old,
                successor: new,
                rights,
            },
            beam::instruction::ChangeAmmV4Creator { revision },
        )
    };
    assert!(
        !send(
            &mut ctx,
            cto(admin.pubkey(), attacker.pubkey(), successor.pubkey(), 0),
            &[&admin, &attacker, &successor],
            5
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            cto(admin.pubkey(), creator.pubkey(), successor.pubkey(), 0),
            &[&admin, &creator, &successor],
            6
        )
        .await
    );
    assert!(
        !send(
            &mut ctx,
            fund(payer, creator.pubkey(), old_vault, CREATOR, 99, 0, None),
            &[],
            7
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            fund(
                payer,
                successor.pubkey(),
                new_vault,
                CREATOR,
                2_000_000,
                1,
                None
            ),
            &[],
            8
        )
        .await
    );

    let claim = |vault, beneficiary| {
        instruction(
            beam::accounts::ClaimAmmV4Vault { vault, beneficiary },
            beam::instruction::ClaimAmmV4Vault {},
        )
    };
    let creator_before = balance(&mut ctx, creator.pubkey()).await;
    assert!(!send(&mut ctx, claim(old_vault, successor.pubkey()), &[], 9).await);
    assert!(send(&mut ctx, claim(old_vault, creator.pubkey()), &[], 10).await);
    assert_eq!(
        balance(&mut ctx, creator.pubkey()).await,
        creator_before + 5_000_000
    );
    assert_eq!(balance(&mut ctx, old_vault).await, vault_rent + 1_000_000);
    assert!(send(&mut ctx, claim(old_vault, creator.pubkey()), &[], 11).await);
    assert_eq!(
        balance(&mut ctx, creator.pubkey()).await,
        creator_before + 5_000_000
    );
    let successor_before = balance(&mut ctx, successor.pubkey()).await;
    assert!(send(&mut ctx, claim(new_vault, successor.pubkey()), &[], 12).await);
    assert_eq!(
        balance(&mut ctx, successor.pubkey()).await,
        successor_before + 2_000_000
    );

    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::RegisterDonationRecipient {
                    global,
                    authority: admin.pubkey(),
                    recipient: recipient.pubkey(),
                    record,
                    system_program: system_program::ID,
                },
                beam::instruction::RegisterDonationRecipient {}
            ),
            &[&admin, &recipient],
            13
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::SelectAmmV4Donation {
                    creator: successor.pubkey(),
                    rights,
                    record,
                },
                beam::instruction::SelectAmmV4Donation { revision: 1 }
            ),
            &[&successor],
            14
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            fund(
                payer,
                recipient.pubkey(),
                donation_vault,
                DONATION,
                3_000_000,
                2,
                Some(record)
            ),
            &[],
            15
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::RevokeDonationRecipient {
                    global,
                    authority: admin.pubkey(),
                    record,
                },
                beam::instruction::RevokeDonationRecipient {}
            ),
            &[&admin],
            16
        )
        .await
    );
    let donation_before = stored(&mut ctx, donation_vault).await;
    assert!(
        !send(
            &mut ctx,
            fund(
                payer,
                recipient.pubkey(),
                donation_vault,
                DONATION,
                100,
                2,
                Some(record)
            ),
            &[],
            17
        )
        .await
    );
    let donation_after = stored(&mut ctx, donation_vault).await;
    assert_eq!(donation_after.lamports, donation_before.lamports);
    assert_eq!(donation_after.data, donation_before.data);
    let recipient_before = balance(&mut ctx, recipient.pubkey()).await;
    assert!(send(&mut ctx, claim(donation_vault, recipient.pubkey()), &[], 18).await);
    assert_eq!(
        balance(&mut ctx, recipient.pubkey()).await,
        recipient_before + 3_000_000
    );

    // A separate configured route charges a single 50-bps Beam fee, with
    // creator fees paid from the community allocation. Raydium is mocked.
    let policy = pda(&[POLICY_SEED, addresses.amm.as_ref()]);
    let community_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[COMMUNITY],
        admin.pubkey().as_ref(),
    ]);
    let treasury_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[TREASURY],
        attacker.pubkey().as_ref(),
    ]);
    let founder_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[FOUNDER],
        creator.pubkey().as_ref(),
    ]);
    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::ConfigureAmmV4Routing {
                    global,
                    authority: admin.pubkey(),
                    creator: successor.pubkey(),
                    rights,
                    policy,
                    system_program: system_program::ID,
                },
                beam::instruction::ConfigureAmmV4Routing {
                    community: admin.pubkey(),
                    treasury: attacker.pubkey(),
                    founder: creator.pubkey(),
                    creator_bps: 30,
                    revision: 2,
                }
            ),
            &[&admin, &successor],
            30
        )
        .await
    );
    let swap = |source, destination, input, minimum, maximum_fee, revision| {
        instruction(
            beam::accounts::SwapAmmV4 {
                user: trader.pubkey(),
                rights,
                policy,
                pool: addresses.amm,
                raydium_program: AMM_PROGRAM,
                amm_authority: addresses.authority,
                coin_vault: addresses.coin_vault,
                pc_vault: addresses.pc_vault,
                source,
                destination,
                creator_vault: new_vault,
                community_vault,
                treasury_vault,
                founder_vault,
                token_program: spl_token::ID,
                system_program: system_program::ID,
            },
            beam::instruction::SwapAmmV4 {
                amount_in: input,
                minimum_out: minimum,
                maximum_beam_fee: maximum_fee,
                revision,
            },
        )
    };
    // Disabled by default. Even init_if_needed account rent is rolled back.
    assert!(
        !send(
            &mut ctx,
            swap(user_quote, user_coin, 1_000_000, 2_000_000, 5_000, 2),
            &[&trader],
            31
        )
        .await
    );
    assert!(ctx
        .banks_client
        .get_account(community_vault)
        .await
        .unwrap()
        .is_none());
    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::EnableAmmV4Routing {
                    global,
                    authority: admin.pubkey(),
                    creator: successor.pubkey(),
                    rights,
                    policy,
                },
                beam::instruction::EnableAmmV4Routing {
                    enabled: true,
                    revision: 2
                }
            ),
            &[&admin, &successor],
            32
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            swap(user_quote, user_coin, 1_000_000, 2_000_000, 5_000, 2),
            &[&trader],
            33
        )
        .await
    );
    assert_eq!(tokens(&mut ctx, user_coin).await, 2_000_000);
    assert_eq!(tokens(&mut ctx, user_quote).await, 99_000_000);
    let before_quote = stored(&mut ctx, user_quote).await;
    let before_coin = stored(&mut ctx, user_coin).await;
    let before_pc = stored(&mut ctx, addresses.pc_vault).await;
    let before_coin_pool = stored(&mut ctx, addresses.coin_vault).await;
    let before_credit = stored(&mut ctx, new_vault).await;
    // A sell's maximum fee is checked against observed quote output after CPI.
    // Rejection must roll back both successful mock token transfers.
    assert!(
        !send(
            &mut ctx,
            swap(user_coin, user_quote, 2_000_000, 1_000_000, 4_999, 2),
            &[&trader],
            34
        )
        .await
    );
    for (key, old) in [
        (user_quote, &before_quote),
        (user_coin, &before_coin),
        (addresses.pc_vault, &before_pc),
        (addresses.coin_vault, &before_coin_pool),
        (new_vault, &before_credit),
    ] {
        let now = stored(&mut ctx, key).await;
        assert_eq!(now.lamports, old.lamports);
        assert_eq!(now.data, old.data);
    }
    // Mock slippage failure occurs after its input transfer and rolls back.
    assert!(
        !send(
            &mut ctx,
            swap(user_quote, user_coin, 1_000_000, 2_000_001, 5_000, 2),
            &[&trader],
            35
        )
        .await
    );
    assert_eq!(tokens(&mut ctx, user_quote).await, 99_000_000);
    assert_eq!(tokens(&mut ctx, user_coin).await, 2_000_000);
    assert!(
        send(
            &mut ctx,
            swap(user_coin, user_quote, 2_000_000, 1_000_000, 5_000, 2),
            &[&trader],
            36
        )
        .await
    );
    assert_eq!(tokens(&mut ctx, user_coin).await, 0);
    assert_eq!(tokens(&mut ctx, user_quote).await, 100_000_000);
    let mut total = 0;
    for (vault, expected) in [
        (new_vault, 6_000),
        (community_vault, 2_400),
        (treasury_vault, 1_000),
        (founder_vault, 600),
    ] {
        let value = stored(&mut ctx, vault).await;
        let state = AmmV4Vault::try_deserialize(&mut value.data.as_slice()).unwrap();
        assert_eq!(state.credited - state.claimed, expected);
        total += expected;
    }
    assert_eq!(total, 10_000); // 2 * 0.50% of 1_000_000, not 0.50% + creator fees.

    // Creator-selected immutable sharing, including cumulative dust handling.
    let sharing = pda(&[SHARING_SEED, addresses.amm.as_ref(), &2u64.to_le_bytes()]);
    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::CreateAmmV4Sharing {
                    creator: successor.pubkey(),
                    rights,
                    plan: sharing,
                    system_program: system_program::ID,
                },
                beam::instruction::CreateAmmV4Sharing {
                    recipients: vec![creator.pubkey(), recipient.pubkey()],
                    basis_points: vec![6000, 4000],
                    revision: 2,
                }
            ),
            &[&successor],
            40
        )
        .await
    );
    let shared_vault = pda(&[
        VAULT_SEED,
        addresses.amm.as_ref(),
        &[CREATOR],
        sharing.as_ref(),
    ]);
    let creator_before_sharing = balance(&mut ctx, creator.pubkey()).await;
    let recipient_before_sharing = balance(&mut ctx, recipient.pubkey()).await;
    assert!(
        send(
            &mut ctx,
            fund(payer, sharing, shared_vault, CREATOR, 1001, 3, None),
            &[],
            41
        )
        .await
    );
    assert!(send(&mut ctx, claim(shared_vault, sharing), &[], 42).await);
    let distribute = || {
        let mut ix = instruction(
            beam::accounts::DistributeAmmV4Sharing { plan: sharing },
            beam::instruction::DistributeAmmV4Sharing {},
        );
        ix.accounts.push(solana_sdk::instruction::AccountMeta::new(
            creator.pubkey(),
            false,
        ));
        ix.accounts.push(solana_sdk::instruction::AccountMeta::new(
            recipient.pubkey(),
            false,
        ));
        ix
    };
    let mut wrong = distribute();
    wrong.accounts[1].pubkey = attacker.pubkey();
    let before_wrong = stored(&mut ctx, sharing).await;
    assert!(!send(&mut ctx, wrong, &[], 43).await);
    let after_wrong = stored(&mut ctx, sharing).await;
    assert_eq!(after_wrong.lamports, before_wrong.lamports);
    assert_eq!(after_wrong.data, before_wrong.data);
    assert!(send(&mut ctx, distribute(), &[], 44).await);
    assert_eq!(
        balance(&mut ctx, creator.pubkey()).await,
        creator_before_sharing + 600
    );
    assert_eq!(
        balance(&mut ctx, recipient.pubkey()).await,
        recipient_before_sharing + 400
    );
    assert!(send(&mut ctx, distribute(), &[], 45).await);
    assert_eq!(
        balance(&mut ctx, creator.pubkey()).await,
        creator_before_sharing + 600
    );
    assert!(
        send(
            &mut ctx,
            fund(payer, sharing, shared_vault, CREATOR, 999, 3, None),
            &[],
            46
        )
        .await
    );
    assert!(send(&mut ctx, claim(shared_vault, sharing), &[], 47).await);
    assert!(send(&mut ctx, distribute(), &[], 48).await);
    assert_eq!(
        balance(&mut ctx, creator.pubkey()).await,
        creator_before_sharing + 1200
    );
    assert_eq!(
        balance(&mut ctx, recipient.pubkey()).await,
        recipient_before_sharing + 800
    );

    // Dedicated admin CTO needs successor consent, never the abandoned
    // creator's key, and cannot redirect prior vault/sharing obligations.
    let takeover = |authority| {
        instruction(
            beam::accounts::TakeoverAmmV4Creator {
                global,
                admin_set_creator_authority: authority,
                successor: creator.pubkey(),
                rights,
            },
            beam::instruction::TakeoverAmmV4Creator {
                expected_creator: successor.pubkey(),
                revision: 3,
            },
        )
    };
    assert!(!send(&mut ctx, takeover(admin.pubkey()), &[&admin, &creator], 50).await);
    let before_takeover = stored(&mut ctx, new_vault).await;
    let sharing_before_takeover = stored(&mut ctx, sharing).await;
    assert!(
        send(
            &mut ctx,
            takeover(cto_admin.pubkey()),
            &[&cto_admin, &creator],
            51
        )
        .await
    );
    let after_takeover = stored(&mut ctx, new_vault).await;
    let sharing_after_takeover = stored(&mut ctx, sharing).await;
    assert_eq!(after_takeover.data, before_takeover.data);
    assert_eq!(after_takeover.lamports, before_takeover.lamports);
    assert_eq!(sharing_after_takeover.data, sharing_before_takeover.data);
    assert_eq!(
        sharing_after_takeover.lamports,
        sharing_before_takeover.lamports
    );
    let updated = stored(&mut ctx, rights).await;
    let updated_rights = AmmV4Rights::try_deserialize(&mut updated.data.as_slice()).unwrap();
    assert_eq!(updated_rights.creator, creator.pubkey());
    assert_eq!(updated_rights.creator_payout, creator.pubkey());
    assert_eq!(updated_rights.revision, 4);

    let update_policy = |who, creator_bps| {
        instruction(
            beam::accounts::EnableAmmV4Routing {
                global,
                authority: admin.pubkey(),
                creator: who,
                rights,
                policy,
            },
            beam::instruction::UpdateAmmV4Routing {
                community: admin.pubkey(),
                treasury: attacker.pubkey(),
                founder: creator.pubkey(),
                creator_bps,
                revision: 4,
            },
        )
    };
    assert!(
        !send(
            &mut ctx,
            update_policy(successor.pubkey(), 20),
            &[&admin, &successor],
            52
        )
        .await
    );
    assert!(
        !send(
            &mut ctx,
            update_policy(creator.pubkey(), 43),
            &[&admin, &creator],
            53
        )
        .await
    );
    assert!(
        send(
            &mut ctx,
            update_policy(creator.pubkey(), 20),
            &[&admin, &creator],
            54
        )
        .await
    );
    let policy_value = stored(&mut ctx, policy).await;
    let policy_state =
        AmmV4RoutingPolicy::try_deserialize(&mut policy_value.data.as_slice()).unwrap();
    assert_eq!(policy_state.creator_bps, 20);
    assert!(!policy_state.enabled);

    assert!(
        send(
            &mut ctx,
            instruction(
                beam::accounts::PauseAmmV4Rights {
                    global,
                    authority: admin.pubkey(),
                    rights,
                },
                beam::instruction::PauseAmmV4Rights {
                    paused: true,
                    revision: 5
                }
            ),
            &[&admin],
            19
        )
        .await
    );
    assert!(
        !send(
            &mut ctx,
            fund(payer, successor.pubkey(), new_vault, CREATOR, 100, 6, None),
            &[],
            20
        )
        .await
    );
    assert!(send(&mut ctx, claim(new_vault, successor.pubkey()), &[], 21).await);
    // Companion operations never touch Raydium pool bytes or principal.
    let pool_after = stored(&mut ctx, addresses.amm).await;
    assert_eq!(pool_after.data, pool_bytes);
    assert_eq!(pool_after.lamports, 100_000_000);
}
