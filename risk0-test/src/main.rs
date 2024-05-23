use {
    log::*,
    solana_measure::measure::Measure,
    solana_rbpf::{
        ebpf::MM_HEAP_START,
        error::{EbpfError, ProgramResult},
        memory_region::MemoryMapping,
        program::{BuiltinFunction, SBPFVersion},
        vm::{Config, ContextObject, EbpfVm},
    },
    solana_sdk::{
        account::AccountSharedData,
        bpf_loader_deprecated,
        feature_set::FeatureSet,
        hash::Hash,
        instruction::{AccountMeta, InstructionError},
        native_loader,
        pubkey::Pubkey,
        saturating_add_assign,
        stable_layout::stable_instruction::StableInstruction,
        transaction_context::{
            IndexOfAccount, InstructionAccount, TransactionAccount, TransactionContext,
        },
    },
    std::{
        alloc::Layout,
        cell::RefCell,
        fmt::{self, Debug},
        rc::Rc,
        sync::{atomic::Ordering, Arc},
    },
};

use risc0_zkvm::guest::env;
use solana_program_runtime::{
    compute_budget::ComputeBudget,
    loaded_programs::{LoadedProgram, LoadedProgramsForTxBatch, ProgramRuntimeEnvironments},
    message_processor::MessageProcessor,
    sysvar_cache::SysvarCache,
    timings::ExecuteTimings,
};
use solana_runtime::{bank::Bank, builtins::BUILTINS};
use solana_sdk::{
    account::ReadableAccount,
    genesis_config::GenesisConfig,
    instruction::Instruction,
    rent::Rent,
    signature::Keypair,
    signer::Signer,
    transaction::{SanitizedTransaction, Transaction, TransactionError},
};

fn main() {
    let tx_accounts: Vec<(Pubkey, AccountSharedData)> = env::read();
    let rent: Rent = env::read();
    let tx: SanitizedTransaction = env::read();
    let slot: u64 = env::read();
    let environments: ProgramRuntimeEnvironments = env::read();
}

fn process_message(
    tx_accounts: Vec<(Pubkey, AccountSharedData)>,
    rent: Rent,
    tx: SanitizedTransaction,
    slot: u64,
    environments: ProgramRuntimeEnvironments,
    program_indices: Vec<Vec<u16>>,
    blockhash: Hash,
    lamports_per_signature: u64,
    feature_set: FeatureSet,
    compute_budget: ComputeBudget,
    sysvar_cache: SysvarCache,
    programs_loaded_for_tx_batch: LoadedProgramsForTxBatch,
) -> (
    u64,
    TransactionContext,
    LoadedProgramsForTxBatch,
    ExecuteTimings,
    Result<(), TransactionError>,
) {
    let mut executed_units = 0u64;
    let mut transaction_context = TransactionContext::new(
        tx_accounts,
        rent,
        compute_budget.max_invoke_stack_height,
        compute_budget.max_instruction_trace_length,
    );
    transaction_context.set_signature(tx.signature());
    let mut programs_modified_by_tx = LoadedProgramsForTxBatch::new(slot, environments);
    let mut timings = ExecuteTimings::default();
    let feature_set = Arc::new(feature_set);
    let process_result = MessageProcessor::process_message(
        tx.message(),
        &program_indices,
        &mut transaction_context,
        None,
        &programs_loaded_for_tx_batch,
        &mut programs_modified_by_tx,
        feature_set,
        compute_budget,
        &mut timings,
        &sysvar_cache,
        blockhash,
        lamports_per_signature,
        &mut executed_units,
    );
    return (
        executed_units,
        transaction_context,
        programs_modified_by_tx,
        timings,
        process_result,
    );
}

fn test() {
    solana_logger::setup();
    let (genesis_config, mint_keypair) = create_genesis_config_no_tx_fee_no_rent(500);
    let (bank, bank_forks) = Bank::new_with_bank_forks_for_tests(&genesis_config);

    let from_pubkey = solana_sdk::pubkey::new_rand();
    let to_pubkey = solana_sdk::pubkey::new_rand();

    let account_metas = vec![
        AccountMeta::new(from_pubkey, false),
        AccountMeta::new(to_pubkey, false),
    ];

    let instruction = Instruction::new_with_bincode(solana_vote_program::id(), &10, account_metas);
    let mut tx = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&mint_keypair.pubkey()),
        &[&mint_keypair],
        bank.last_blockhash(),
    );

    tx.message.account_keys.push(solana_sdk::pubkey::new_rand());

    let slot = bank.slot().saturating_add(1);
    let mut bank = Bank::new_from_parent(bank, &Pubkey::default(), slot);

    for builtin in BUILTINS.iter() {
        if let Some(feature_id) = builtin.feature_id {
            let should_apply_action_for_feature_transition =
                bank.feature_set.is_active(&feature_id);

            if should_apply_action_for_feature_transition {
                bank.add_builtin(
                    builtin.program_id,
                    builtin.name.to_string(),
                    LoadedProgram::new_builtin(
                        bank.feature_set.activated_slot(&feature_id).unwrap_or(0),
                        builtin.name.len(),
                        builtin.entrypoint,
                    ),
                );
            }
        }
    }
    let bank = bank_forks
        .write()
        .unwrap()
        .insert(bank)
        .clone_without_scheduler();
    let result = bank.process_transaction(&tx);
    let accounts = bank.get_all_accounts().unwrap();
    println!("{:?}", accounts.len());
}

fn create_genesis_config_no_tx_fee_no_rent(lamports: u64) -> (GenesisConfig, Keypair) {
    // genesis_util creates config with no tx fee and no rent
    let genesis_config_info = solana_runtime::genesis_utils::create_genesis_config(lamports);
    (
        genesis_config_info.genesis_config,
        genesis_config_info.mint_keypair,
    )
}
