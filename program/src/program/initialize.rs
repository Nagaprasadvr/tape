use tape_api::prelude::*;
use solana_program::{program_pack::Pack, program::{invoke, invoke_signed}};
use spl_token::state::Mint;
use steel::*;

pub fn process_initialize(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    let [
        signer_info, 
        archive_info, 
        epoch_info, 
        block_info,
        metadata_info, 
        mint_info, 
        treasury_info, 
        treasury_ata_info, 
        tape_info,
        writer_info,
        tape_program_info,
        system_program_info, 
        token_program_info, 
        associated_token_program_info, 
        metadata_program_info, 
        rent_sysvar_info,
        slot_hashes_info,
        _rest@..
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    archive_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[ARCHIVE], &tape_api::ID)?;

    epoch_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[EPOCH], &tape_api::ID)?;

    block_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[BLOCK], &tape_api::ID)?;

    // Check mint, metadata, treasury
    let (mint_address, mint_bump) = mint_pda();
    let (treasury_address, treasury_bump) = treasury_pda();
    let (metadata_address, _metadata_bump) = metadata_pda(mint_address);

    assert_eq!(mint_bump, MINT_BUMP);
    assert_eq!(treasury_bump, TREASURY_BUMP);

    mint_info
        .is_empty()?
        .is_writable()?
        .has_address(&mint_address)?;

    metadata_info
        .is_empty()?
        .is_writable()?
        .has_address(&metadata_address)?;

    treasury_info
        .is_empty()?
        .is_writable()?
        .has_address(&treasury_address)?;

    treasury_ata_info
        .is_empty()?
        .is_writable()?;

    // Check programs and sysvars.
    tape_program_info
        .is_program(&tape_api::ID)?;
    system_program_info
        .is_program(&system_program::ID)?;
    token_program_info
        .is_program(&spl_token::ID)?;
    associated_token_program_info
        .is_program(&spl_associated_token_account::ID)?;

    metadata_program_info
        .is_program(&mpl_token_metadata::ID)?;
    rent_sysvar_info
        .is_sysvar(&sysvar::rent::ID)?;

    // Initialize epoch.
    create_program_account::<Epoch>(
        epoch_info,
        system_program_info,
        signer_info,
        &tape_api::ID,
        &[EPOCH],
    )?;

    let epoch = epoch_info.as_account_mut::<Epoch>(&tape_api::ID)?;

    epoch.number               = 1;
    epoch.progress             = 0;
    epoch.target_participation = MIN_PARTICIPATION_TARGET;
    epoch.target_difficulty    = MIN_DIFFICULTY;
    epoch.reward_rate          = get_base_rate(1);
    epoch.duplicates           = 0;
    epoch.last_epoch_at        = 0;

    // Initialize block.
    create_program_account::<Block>(
        block_info,
        system_program_info,
        signer_info,
        &tape_api::ID,
        &[BLOCK],
    )?;

    let block = block_info.as_account_mut::<Block>(&tape_api::ID)?;

    block.number            = 1;
    block.progress          = 0;
    block.last_proof_at     = 0;
    block.last_block_at     = 0;

    let next_challenge = compute_next_challenge(
        &BLOCK_ADDRESS.to_bytes(),
        slot_hashes_info
    );

    block.challenge = next_challenge;
    block.challenge_set = 1;

    // Initialize archive.
    create_program_account::<Archive>(
        archive_info,
        system_program_info,
        signer_info,
        &tape_api::ID,
        &[ARCHIVE],
    )?;

    let archive = archive_info.as_account_mut::<Archive>(&tape_api::ID)?;

    archive.tapes_stored = 0;
    archive.bytes_stored = 0;

    // Initialize treasury.
    create_program_account::<Treasury>(
        treasury_info,
        system_program_info,
        signer_info,
        &tape_api::ID,
        &[TREASURY],
    )?;

    // Initialize mint.
    allocate_account_with_bump(
        mint_info,
        system_program_info,
        signer_info,
        Mint::LEN,
        &spl_token::ID,
        &[MINT, MINT_SEED],
        MINT_BUMP,
    )?;
    initialize_mint_signed_with_bump(
        mint_info,
        treasury_info,
        None,
        token_program_info,
        rent_sysvar_info,
        TOKEN_DECIMALS,
        &[MINT, MINT_SEED],
        MINT_BUMP,
    )?;

    // Initialize mint metadata.
    mpl_token_metadata::instructions::CreateMetadataAccountV3Cpi {
        __program: metadata_program_info,
        metadata: metadata_info,
        mint: mint_info,
        mint_authority: treasury_info,
        payer: signer_info,
        update_authority: (signer_info, true),
        system_program: system_program_info,
        rent: Some(rent_sysvar_info),
        __args: mpl_token_metadata::instructions::CreateMetadataAccountV3InstructionArgs {
            data: mpl_token_metadata::types::DataV2 {
                name: METADATA_NAME.to_string(),
                symbol: METADATA_SYMBOL.to_string(),
                uri: METADATA_URI.to_string(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        },
    }
    .invoke_signed(&[&[TREASURY, &[TREASURY_BUMP]]])?;

    // Initialize treasury token account.
    create_associated_token_account(
        signer_info,
        treasury_info,
        treasury_ata_info,
        mint_info,
        system_program_info,
        token_program_info,
        associated_token_program_info,
    )?;

    // Fund the treasury token account.
    mint_to_signed(
        mint_info,
        treasury_ata_info,
        treasury_info,
        token_program_info,
        MAX_SUPPLY,
        &[TREASURY],
    )?;

    // Create the genesis tape

    let name = "genesis";
    let (tape_address, _tape_bump) = tape_pda(*signer_info.key, &to_name(name));
    let (writer_address, _writer_bump) = writer_pda(tape_address);

    // Create the tape
    invoke(
        &build_create_ix(
            *signer_info.key,
            name,
        ),
        &[
            signer_info.clone(),
            tape_info.clone(),
            writer_info.clone(),
            system_program_info.clone(),
            rent_sysvar_info.clone(),
            slot_hashes_info.clone(),
        ],
    )?;

    // Write "hello, world" to the tape
    invoke(
        &build_write_ix(
            *signer_info.key,
            tape_address,
            writer_address,
            b"hello, world",
        ),
        &[
            signer_info.clone(),
            tape_info.clone(),
            writer_info.clone(),
        ],
    )?;

    // Subsidize the tape for 1 block
    invoke_signed(
        &build_subsidize_ix(
            *treasury_info.key,
            *treasury_ata_info.key,
            tape_address,
            min_finalization_rent(1),
        ),
        &[
            treasury_info.clone(),
            treasury_ata_info.clone(),
            tape_info.clone(),
        ],
        &[&[TREASURY, &[TREASURY_BUMP]]]
    )?;

    // Finalize the tape
    invoke(
        &build_finalize_ix(
            *signer_info.key,
            tape_address,
            writer_address,
        ),
        &[
            signer_info.clone(),
            tape_info.clone(),
            writer_info.clone(),
            archive_info.clone(),
            system_program_info.clone(),
            rent_sysvar_info.clone(),
        ],
    )?;

    Ok(())
}
