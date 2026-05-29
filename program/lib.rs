
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer, Mint};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs");

const MAX_PLAYERS: usize       = 4;
const MAX_GAME_ID_LEN: usize   = 32;
const ESCROW_SEED: &[u8]       = b"escrow";
const VAULT_SEED:  &[u8]       = b"vault";
const TOKEN_ESCROW_SEED: &[u8] = b"token_escrow";
const TOKEN_VAULT_SEED:  &[u8] = b"token_vault";

const EMERGENCY_TIMEOUT_SECS: i64 = 24 * 60 * 60;

#[program]
pub mod cryptan_escrow {
    use super::*;


    pub fn initialize_game(
        ctx:           Context<InitializeGame>,
        game_id:       String,
        entry_fee:     u64,
        max_players:   u8,
        house_fee_bps: u16,
    ) -> Result<()> {
        require!(game_id.len() >= 1 && game_id.len() <= MAX_GAME_ID_LEN, EscrowError::InvalidGameId);
        require!(max_players >= 2 && max_players <= 64,                    EscrowError::InvalidMaxPlayers);
        require!(house_fee_bps <= 2_000,                                   EscrowError::HouseFeeTooHigh);
        require!(entry_fee > 0,                                            EscrowError::InvalidEntryFee);

        let escrow = &mut ctx.accounts.escrow;
        escrow.authority     = ctx.accounts.authority.key();
        escrow.game_id       = game_id.clone();
        escrow.entry_fee     = entry_fee;
        escrow.max_players   = max_players;
        escrow.player_count  = 0;
        escrow.house_fee_bps = house_fee_bps;
        escrow.players       = [Pubkey::default(); MAX_PLAYERS];
        escrow.status        = GameStatus::Open;
        escrow.bump          = ctx.bumps.escrow;
        escrow.vault_bump    = ctx.bumps.vault;
        escrow.created_at    = Clock::get()?.unix_timestamp;

        emit!(GameInitialized { game_id, entry_fee, max_players, house_fee_bps });
        Ok(())
    }

    pub fn deposit<'info>(ctx: Context<'_, '_, '_, 'info, Deposit<'info>>, game_id: String) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;

        require!(escrow.status == GameStatus::Open,            EscrowError::GameNotOpen);
        require!((escrow.player_count as usize) < MAX_PLAYERS, EscrowError::GameFull);

        let player_key = ctx.accounts.player.key();
        for i in 0..(escrow.player_count as usize) {
            require!(escrow.players[i] != player_key, EscrowError::AlreadyDeposited);
        }

        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &player_key,
                ctx.accounts.vault.key,
                escrow.entry_fee,
            ),
            &[
                ctx.accounts.player.to_account_info(),
                ctx.accounts.vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        let slot = escrow.player_count as usize;
        escrow.players[slot] = player_key;
        escrow.player_count += 1;

        emit!(PlayerDeposited { game_id, player: player_key, player_count: escrow.player_count });
        Ok(())
    }

    pub fn finalize<'info>(
        ctx:     Context<'_, '_, '_, 'info, Finalize<'info>>,
        game_id: String,
        payouts: Vec<PayoutEntry>,
    ) -> Result<()> {
        let escrow = &ctx.accounts.escrow;

        require!(escrow.status != GameStatus::Finalized &&
                 escrow.status != GameStatus::Cancelled,  EscrowError::AlreadyFinalized);
        require!(!payouts.is_empty(),                     EscrowError::EmptyPayouts);

        let total_bps: u32 = payouts.iter().map(|p| p.basis_points as u32).sum();
        require!(total_bps == 10_000, EscrowError::InvalidPayoutSplit);

        let game_id_bytes = game_id.as_bytes();
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, game_id_bytes, &[escrow.vault_bump]];
        let signer_seeds = &[vault_seeds];

        let vault_lamports = ctx.accounts.vault.lamports();
        let house_fee = vault_lamports
            .checked_mul(escrow.house_fee_bps as u64).ok_or(EscrowError::Overflow)?
            .checked_div(10_000).ok_or(EscrowError::Overflow)?;
        let distributable = vault_lamports.checked_sub(house_fee).ok_or(EscrowError::Overflow)?;

        let remaining = ctx.remaining_accounts;
        require!(remaining.len() >= payouts.len() + 1, EscrowError::MissingAccount);

        let vault_info  = ctx.accounts.vault.clone();
        let system_info = ctx.accounts.system_program.to_account_info();

        for (i, payout) in payouts.iter().enumerate() {
            let idx = payout.player_index as usize;
            require!(idx < escrow.player_count as usize, EscrowError::InvalidPlayerIndex);

            let amount = distributable
                .checked_mul(payout.basis_points as u64).ok_or(EscrowError::Overflow)?
                .checked_div(10_000).ok_or(EscrowError::Overflow)?;

            let recipient = &remaining[i];
            require!(recipient.key() == escrow.players[idx], EscrowError::MissingAccount);

            anchor_lang::solana_program::program::invoke_signed(
                &anchor_lang::solana_program::system_instruction::transfer(
                    ctx.accounts.vault.key, recipient.key, amount,
                ),
                &[vault_info.clone(), recipient.clone(), system_info.clone()],
                signer_seeds,
            )?;
        }

        let authority_acc = remaining.last().unwrap();
        anchor_lang::solana_program::program::invoke_signed(
            &anchor_lang::solana_program::system_instruction::transfer(
                ctx.accounts.vault.key, authority_acc.key, house_fee,
            ),
            &[vault_info.clone(), authority_acc.clone(), system_info.clone()],
            signer_seeds,
        )?;

        let escrow = &mut ctx.accounts.escrow;
        escrow.status = GameStatus::Finalized;

        emit!(GameFinalized { game_id, total_pool: vault_lamports, house_fee, distributable });
        Ok(())
    }

    pub fn cancel<'info>(ctx: Context<'_, '_, '_, 'info, Cancel<'info>>, game_id: String) -> Result<()> {
        let escrow = &ctx.accounts.escrow;

        require!(escrow.status != GameStatus::Finalized &&
                 escrow.status != GameStatus::Cancelled, EscrowError::AlreadyFinalized);

        let game_id_bytes = game_id.as_bytes();
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, game_id_bytes, &[escrow.vault_bump]];
        let signer_seeds = &[vault_seeds];

        let remaining = ctx.remaining_accounts;
        let vault_info  = ctx.accounts.vault.clone();
        let system_info = ctx.accounts.system_program.to_account_info();

        for i in 0..(escrow.player_count as usize) {
            require!(i < remaining.len(), EscrowError::MissingAccount);
            let recipient = &remaining[i];
            require!(recipient.key() == escrow.players[i], EscrowError::MissingAccount);

            anchor_lang::solana_program::program::invoke_signed(
                &anchor_lang::solana_program::system_instruction::transfer(
                    ctx.accounts.vault.key, recipient.key, escrow.entry_fee,
                ),
                &[vault_info.clone(), recipient.clone(), system_info.clone()],
                signer_seeds,
            )?;
        }

        let escrow = &mut ctx.accounts.escrow;
        escrow.status = GameStatus::Cancelled;

        emit!(GameCancelled { game_id });
        Ok(())
    }

    pub fn emergency_refund<'info>(
        ctx:     Context<'_, '_, '_, 'info, EmergencyRefund<'info>>,
        game_id: String,
    ) -> Result<()> {
        let escrow = &ctx.accounts.escrow;

        require!(
            escrow.status != GameStatus::Finalized &&
            escrow.status != GameStatus::Cancelled,
            EscrowError::AlreadyFinalized
        );

        let now = Clock::get()?.unix_timestamp;
        require!(
            now >= escrow.created_at + EMERGENCY_TIMEOUT_SECS,
            EscrowError::TimelockNotExpired
        );

        let caller = ctx.accounts.player.key();
        let player_idx = escrow.players[..escrow.player_count as usize]
            .iter()
            .position(|pk| *pk == caller)
            .ok_or(EscrowError::NotAPlayer)?;

        let game_id_bytes = game_id.as_bytes();
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, game_id_bytes, &[escrow.vault_bump]];
        let signer_seeds = &[vault_seeds];

        let vault_info  = ctx.accounts.vault.clone();
        let player_info = ctx.accounts.player.to_account_info();
        let system_info = ctx.accounts.system_program.to_account_info();

        anchor_lang::solana_program::program::invoke_signed(
            &anchor_lang::solana_program::system_instruction::transfer(
                ctx.accounts.vault.key,
                &caller,
                escrow.entry_fee,
            ),
            &[vault_info, player_info, system_info],
            signer_seeds,
        )?;

        emit!(EmergencyRefunded {
            game_id,
            player:       caller,
            player_index: player_idx as u8,
            amount:       escrow.entry_fee,
        });
        Ok(())
    }


    pub fn force_cancel<'info>(
        ctx:     Context<'_, '_, '_, 'info, ForceCancel<'info>>,
        game_id: String,
    ) -> Result<()> {
        let escrow = &ctx.accounts.escrow;

        require!(
            escrow.status != GameStatus::Finalized &&
            escrow.status != GameStatus::Cancelled,
            EscrowError::AlreadyFinalized
        );

        let now = Clock::get()?.unix_timestamp;
        require!(
            now >= escrow.created_at + EMERGENCY_TIMEOUT_SECS,
            EscrowError::TimelockNotExpired
        );

        let caller = ctx.accounts.caller.key();
        let is_player = escrow.players[..escrow.player_count as usize]
            .iter()
            .any(|pk| *pk == caller);
        require!(is_player, EscrowError::NotAPlayer);

        let player_count = escrow.player_count as usize;
        let entry_fee    = escrow.entry_fee;
        let vault_bump   = escrow.vault_bump;

        let game_id_bytes = game_id.as_bytes();
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, game_id_bytes, &[vault_bump]];
        let signer_seeds = &[vault_seeds];

        let remaining   = ctx.remaining_accounts;
        require!(remaining.len() >= player_count, EscrowError::MissingAccount);

        let vault_info  = ctx.accounts.vault.clone();
        let system_info = ctx.accounts.system_program.to_account_info();

        for i in 0..player_count {
            let recipient = &remaining[i];
            require!(recipient.key() == escrow.players[i], EscrowError::MissingAccount);

            anchor_lang::solana_program::program::invoke_signed(
                &anchor_lang::solana_program::system_instruction::transfer(
                    ctx.accounts.vault.key,
                    recipient.key,
                    entry_fee,
                ),
                &[vault_info.clone(), recipient.clone(), system_info.clone()],
                signer_seeds,
            )?;
        }

        let escrow = &mut ctx.accounts.escrow;
        escrow.status = GameStatus::Cancelled;

        emit!(GameCancelled { game_id });
        Ok(())
    }


    pub fn initialize_token_game(
        ctx:           Context<InitializeTokenGame>,
        game_id:       String,
        entry_fee:     u64,
        max_players:   u8,
        house_fee_bps: u16,
    ) -> Result<()> {
        require!(game_id.len() >= 1 && game_id.len() <= MAX_GAME_ID_LEN, EscrowError::InvalidGameId);
        require!(max_players >= 2 && max_players <= 64,                    EscrowError::InvalidMaxPlayers);
        require!(house_fee_bps <= 2_000,                                   EscrowError::HouseFeeTooHigh);
        require!(entry_fee > 0,                                            EscrowError::InvalidEntryFee);

        let escrow = &mut ctx.accounts.escrow;
        escrow.authority     = ctx.accounts.authority.key();
        escrow.game_id       = game_id.clone();
        escrow.token_mint    = ctx.accounts.token_mint.key();
        escrow.entry_fee     = entry_fee;
        escrow.max_players   = max_players;
        escrow.player_count  = 0;
        escrow.house_fee_bps = house_fee_bps;
        escrow.players       = [Pubkey::default(); MAX_PLAYERS];
        escrow.status        = GameStatus::Open;
        escrow.bump          = ctx.bumps.escrow;
        escrow.vault_bump    = ctx.bumps.token_vault;
        escrow.created_at    = Clock::get()?.unix_timestamp;

        emit!(GameInitialized { game_id, entry_fee, max_players, house_fee_bps });
        Ok(())
    }

    pub fn token_deposit(ctx: Context<TokenDeposit>, game_id: String) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;

        require!(escrow.status == GameStatus::Open,            EscrowError::GameNotOpen);
        require!((escrow.player_count as usize) < MAX_PLAYERS, EscrowError::GameFull);
        require!(ctx.accounts.token_mint.key() == escrow.token_mint, EscrowError::WrongMint);

        let player_key = ctx.accounts.player.key();
        for i in 0..(escrow.player_count as usize) {
            require!(escrow.players[i] != player_key, EscrowError::AlreadyDeposited);
        }

        let transfer_amount = escrow.entry_fee;

        let cpi_accounts = Transfer {
            from:      ctx.accounts.player_ata.to_account_info(),
            to:        ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        token::transfer(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts),
            transfer_amount,
        )?;

        let slot = escrow.player_count as usize;
        escrow.players[slot] = player_key;
        escrow.player_count += 1;

        emit!(PlayerDeposited { game_id, player: player_key, player_count: escrow.player_count });
        Ok(())
    }

    pub fn token_finalize<'info>(
        ctx:     Context<'_, '_, 'info, 'info, TokenFinalize<'info>>,
        game_id: String,
        payouts: Vec<PayoutEntry>,
    ) -> Result<()> {
        let status        = ctx.accounts.escrow.status.clone();
        let house_fee_bps = ctx.accounts.escrow.house_fee_bps;
        let player_count  = ctx.accounts.escrow.player_count;
        let escrow_bump   = ctx.accounts.escrow.bump;
        let token_mint    = ctx.accounts.escrow.token_mint;
        let players: [Pubkey; MAX_PLAYERS] = ctx.accounts.escrow.players;

        require!(status != GameStatus::Finalized && status != GameStatus::Cancelled, EscrowError::AlreadyFinalized);
        require!(!payouts.is_empty(),   EscrowError::EmptyPayouts);

        let total_bps: u32 = payouts.iter().map(|p| p.basis_points as u32).sum();
        require!(total_bps == 10_000,   EscrowError::InvalidPayoutSplit);

        let game_id_bytes = game_id.as_bytes();
        let escrow_seeds: &[&[u8]] = &[TOKEN_ESCROW_SEED, game_id_bytes, &[escrow_bump]];
        let signer_seeds = &[escrow_seeds];

        let vault_balance = ctx.accounts.token_vault.amount;
        let house_fee = vault_balance
            .checked_mul(house_fee_bps as u64).ok_or(EscrowError::Overflow)?
            .checked_div(10_000).ok_or(EscrowError::Overflow)?;
        let distributable = vault_balance.checked_sub(house_fee).ok_or(EscrowError::Overflow)?;

        let remaining = ctx.remaining_accounts;
        require!(remaining.len() >= payouts.len() + 1, EscrowError::MissingAccount);

        for (i, payout) in payouts.iter().enumerate() {
            let idx = payout.player_index as usize;
            require!(idx < player_count as usize, EscrowError::InvalidPlayerIndex);

            let amount = distributable
                .checked_mul(payout.basis_points as u64).ok_or(EscrowError::Overflow)?
                .checked_div(10_000).ok_or(EscrowError::Overflow)?;

            let recipient_info = &remaining[i];
            let recipient_ta = Account::<TokenAccount>::try_from(recipient_info)
                .map_err(|_| EscrowError::MissingAccount)?;
            require!(recipient_ta.owner == players[idx],  EscrowError::MissingAccount);
            require!(recipient_ta.mint  == token_mint,    EscrowError::WrongMint);
            drop(recipient_ta);

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from:      ctx.accounts.token_vault.to_account_info(),
                        to:        recipient_info.clone(),
                        authority: ctx.accounts.escrow.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount,
            )?;
        }

        if house_fee > 0 {
            let house_ata = remaining.last().unwrap();
            let house_ta = Account::<TokenAccount>::try_from(house_ata)
                .map_err(|_| EscrowError::MissingAccount)?;
            require!(house_ta.mint == token_mint, EscrowError::WrongMint);
            drop(house_ta);

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from:      ctx.accounts.token_vault.to_account_info(),
                        to:        house_ata.clone(),
                        authority: ctx.accounts.escrow.to_account_info(),
                    },
                    signer_seeds,
                ),
                house_fee,
            )?;
        }

        ctx.accounts.escrow.status = GameStatus::Finalized;

        emit!(GameFinalized { game_id, total_pool: vault_balance, house_fee, distributable });
        Ok(())
    }

    pub fn token_cancel<'info>(
        ctx:     Context<'_, '_, 'info, 'info, TokenCancel<'info>>,
        game_id: String,
    ) -> Result<()> {
        let status       = ctx.accounts.escrow.status.clone();
        let player_count = ctx.accounts.escrow.player_count;
        let entry_fee    = ctx.accounts.escrow.entry_fee;
        let escrow_bump  = ctx.accounts.escrow.bump;
        let token_mint   = ctx.accounts.escrow.token_mint;
        let players: [Pubkey; MAX_PLAYERS] = ctx.accounts.escrow.players;

        require!(status != GameStatus::Finalized && status != GameStatus::Cancelled, EscrowError::AlreadyFinalized);

        let game_id_bytes = game_id.as_bytes();
        let escrow_seeds: &[&[u8]] = &[TOKEN_ESCROW_SEED, game_id_bytes, &[escrow_bump]];
        let signer_seeds = &[escrow_seeds];

        let remaining = ctx.remaining_accounts;

        for i in 0..(player_count as usize) {
            require!(i < remaining.len(), EscrowError::MissingAccount);

            let recipient_info = &remaining[i];
            let recipient_ta = Account::<TokenAccount>::try_from(recipient_info)
                .map_err(|_| EscrowError::MissingAccount)?;
            require!(recipient_ta.owner == players[i], EscrowError::MissingAccount);
            require!(recipient_ta.mint  == token_mint, EscrowError::WrongMint);
            drop(recipient_ta);

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from:      ctx.accounts.token_vault.to_account_info(),
                        to:        recipient_info.clone(),
                        authority: ctx.accounts.escrow.to_account_info(),
                    },
                    signer_seeds,
                ),
                entry_fee,
            )?;
        }

        ctx.accounts.escrow.status = GameStatus::Cancelled;

        emit!(GameCancelled { game_id });
        Ok(())
    }

    pub fn token_emergency_refund(
        ctx:     Context<TokenEmergencyRefund>,
        game_id: String,
    ) -> Result<()> {
        let status       = ctx.accounts.escrow.status.clone();
        let created_at   = ctx.accounts.escrow.created_at;
        let entry_fee    = ctx.accounts.escrow.entry_fee;
        let player_count = ctx.accounts.escrow.player_count;
        let escrow_bump  = ctx.accounts.escrow.bump;
        let token_mint   = ctx.accounts.escrow.token_mint;

        require!(status != GameStatus::Finalized && status != GameStatus::Cancelled, EscrowError::AlreadyFinalized);

        let now = Clock::get()?.unix_timestamp;
        require!(now >= created_at + EMERGENCY_TIMEOUT_SECS, EscrowError::TimelockNotExpired);

        let caller = ctx.accounts.player.key();
        let player_idx = ctx.accounts.escrow.players[..player_count as usize]
            .iter()
            .position(|pk| *pk == caller)
            .ok_or(EscrowError::NotAPlayer)?;

        require!(ctx.accounts.player_ata.mint  == token_mint, EscrowError::WrongMint);
        require!(ctx.accounts.player_ata.owner == caller,     EscrowError::NotAPlayer);

        let game_id_bytes = game_id.as_bytes();
        let escrow_seeds: &[&[u8]] = &[TOKEN_ESCROW_SEED, game_id_bytes, &[escrow_bump]];
        let signer_seeds = &[escrow_seeds];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from:      ctx.accounts.token_vault.to_account_info(),
                    to:        ctx.accounts.player_ata.to_account_info(),
                    authority: ctx.accounts.escrow.to_account_info(),
                },
                signer_seeds,
            ),
            entry_fee,
        )?;

        emit!(EmergencyRefunded {
            game_id,
            player:       caller,
            player_index: player_idx as u8,
            amount:       entry_fee,
        });
        Ok(())
    }

    pub fn force_token_cancel<'info>(
        ctx:     Context<'_, '_, 'info, 'info, ForceTokenCancel<'info>>,
        game_id: String,
    ) -> Result<()> {
        let status       = ctx.accounts.escrow.status.clone();
        let created_at   = ctx.accounts.escrow.created_at;
        let entry_fee    = ctx.accounts.escrow.entry_fee;
        let player_count = ctx.accounts.escrow.player_count;
        let escrow_bump  = ctx.accounts.escrow.bump;
        let token_mint   = ctx.accounts.escrow.token_mint;
        let players: [Pubkey; MAX_PLAYERS] = ctx.accounts.escrow.players;

        require!(
            status != GameStatus::Finalized && status != GameStatus::Cancelled,
            EscrowError::AlreadyFinalized
        );

        let now = Clock::get()?.unix_timestamp;
        require!(now >= created_at + EMERGENCY_TIMEOUT_SECS, EscrowError::TimelockNotExpired);

        let caller = ctx.accounts.caller.key();
        let is_player = players[..player_count as usize].iter().any(|pk| *pk == caller);
        require!(is_player, EscrowError::NotAPlayer);

        let game_id_bytes = game_id.as_bytes();
        let escrow_seeds: &[&[u8]] = &[TOKEN_ESCROW_SEED, game_id_bytes, &[escrow_bump]];
        let signer_seeds = &[escrow_seeds];

        let remaining = ctx.remaining_accounts;
        require!(remaining.len() >= player_count as usize, EscrowError::MissingAccount);

        for i in 0..(player_count as usize) {
            let recipient_info = &remaining[i];
            let recipient_ta = Account::<TokenAccount>::try_from(recipient_info)
                .map_err(|_| EscrowError::MissingAccount)?;
            require!(recipient_ta.owner == players[i], EscrowError::MissingAccount);
            require!(recipient_ta.mint  == token_mint, EscrowError::WrongMint);
            drop(recipient_ta);

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from:      ctx.accounts.token_vault.to_account_info(),
                        to:        recipient_info.clone(),
                        authority: ctx.accounts.escrow.to_account_info(),
                    },
                    signer_seeds,
                ),
                entry_fee,
            )?;
        }

        ctx.accounts.escrow.status = GameStatus::Cancelled;
        emit!(GameCancelled { game_id });
        Ok(())
    }

}


#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct InitializeGame<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer  = authority,
        space  = GameEscrow::space(&game_id),
        seeds  = [ESCROW_SEED, game_id.as_bytes()],
        bump,
    )]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(mut, seeds = [ESCROW_SEED, game_id.as_bytes()], bump = escrow.bump)]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump = escrow.vault_bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct Finalize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds   = [ESCROW_SEED, game_id.as_bytes()],
        bump    = escrow.bump,
        has_one = authority @ EscrowError::Unauthorized,
    )]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump = escrow.vault_bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct Cancel<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds   = [ESCROW_SEED, game_id.as_bytes()],
        bump    = escrow.bump,
        has_one = authority @ EscrowError::Unauthorized,
    )]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump = escrow.vault_bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct EmergencyRefund<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(mut, seeds = [ESCROW_SEED, game_id.as_bytes()], bump = escrow.bump)]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump = escrow.vault_bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct ForceCancel<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,

    #[account(mut, seeds = [ESCROW_SEED, game_id.as_bytes()], bump = escrow.bump)]
    pub escrow: Account<'info, GameEscrow>,

    #[account(mut, seeds = [VAULT_SEED, game_id.as_bytes()], bump = escrow.vault_bump)]
    pub vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct InitializeTokenGame<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = TokenGameEscrow::space(&game_id),
        seeds = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        init,
        payer = authority,
        seeds = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    pub token_program:  Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent:           Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct TokenDeposit<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump  = escrow.bump,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        mut,
        seeds            = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump             = escrow.vault_bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint      = token_mint,
        associated_token::authority = player,
    )]
    pub player_ata: Account<'info, TokenAccount>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program:           Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct TokenFinalize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds   = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump    = escrow.bump,
        has_one = authority @ EscrowError::Unauthorized,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        mut,
        seeds            = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump             = escrow.vault_bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    pub token_program:  Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct TokenCancel<'info> {
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds   = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump    = escrow.bump,
        has_one = authority @ EscrowError::Unauthorized,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        mut,
        seeds            = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump             = escrow.vault_bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    pub token_program:  Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct TokenEmergencyRefund<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump  = escrow.bump,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        mut,
        seeds            = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump             = escrow.vault_bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint      = token_mint,
        associated_token::authority = player,
    )]
    pub player_ata: Account<'info, TokenAccount>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program:           Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(game_id: String)]
pub struct ForceTokenCancel<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [TOKEN_ESCROW_SEED, game_id.as_bytes()],
        bump  = escrow.bump,
    )]
    pub escrow: Account<'info, TokenGameEscrow>,

    #[account(
        mut,
        seeds            = [TOKEN_VAULT_SEED, game_id.as_bytes()],
        bump             = escrow.vault_bump,
        token::mint      = token_mint,
        token::authority = escrow,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    pub token_program:  Program<'info, Token>,
    pub system_program: Program<'info, System>,
}


#[account]
pub struct GameEscrow {
    pub authority:     Pubkey,
    pub game_id:       String,
    pub entry_fee:     u64,
    pub max_players:   u8,
    pub player_count:  u8,
    pub house_fee_bps: u16,
    pub players:       [Pubkey; MAX_PLAYERS],
    pub status:        GameStatus,
    pub bump:          u8,
    pub vault_bump:    u8,
    pub created_at:    i64,
}

impl GameEscrow {
    pub fn space(game_id: &str) -> usize {
        8
        + 32
        + 4 + game_id.len().max(MAX_GAME_ID_LEN)
        + 8 + 1 + 1 + 2
        + 32 * MAX_PLAYERS
        + 1 + 1 + 1 + 8
    }
}

#[account]
pub struct TokenGameEscrow {
    pub authority:     Pubkey,
    pub game_id:       String,
    pub token_mint:    Pubkey,
    pub entry_fee:     u64,
    pub max_players:   u8,
    pub player_count:  u8,
    pub house_fee_bps: u16,
    pub players:       [Pubkey; MAX_PLAYERS],
    pub status:        GameStatus,
    pub bump:          u8,
    pub vault_bump:    u8,
    pub created_at:    i64,
}

impl TokenGameEscrow {
    pub fn space(game_id: &str) -> usize {
        8
        + 32
        + 4 + game_id.len().max(MAX_GAME_ID_LEN)
        + 32
        + 8 + 1 + 1 + 2
        + 32 * MAX_PLAYERS
        + 1 + 1 + 1 + 8
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum GameStatus {
    Open,
    Active,
    Finalized,
    Cancelled,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PayoutEntry {
    pub player_index:  u8,
    pub basis_points:  u16,
}


#[event] pub struct GameInitialized  { pub game_id: String, pub entry_fee: u64, pub max_players: u8, pub house_fee_bps: u16 }
#[event] pub struct PlayerDeposited  { pub game_id: String, pub player: Pubkey, pub player_count: u8 }
#[event] pub struct GameFinalized    { pub game_id: String, pub total_pool: u64, pub house_fee: u64, pub distributable: u64 }
#[event] pub struct GameCancelled    { pub game_id: String }
#[event] pub struct EmergencyRefunded {
    pub game_id:      String,
    pub player:       Pubkey,
    pub player_index: u8,
    pub amount:       u64,
}


#[error_code]
pub enum EscrowError {
    #[msg("Game ID must be 1-32 characters")]           InvalidGameId,
    #[msg("Max players must be between 2 and 64")]      InvalidMaxPlayers,
    #[msg("House fee cannot exceed 20%")]               HouseFeeTooHigh,
    #[msg("Entry fee must be greater than 0")]          InvalidEntryFee,
    #[msg("Game is not open for deposits")]             GameNotOpen,
    #[msg("Game lobby is full")]                        GameFull,
    #[msg("Wallet already deposited")]                  AlreadyDeposited,
    #[msg("Game already finalized or cancelled")]       AlreadyFinalized,
    #[msg("Payout list is empty")]                      EmptyPayouts,
    #[msg("Payout basis points must sum to 10000")]     InvalidPayoutSplit,
    #[msg("Player index out of range")]                 InvalidPlayerIndex,
    #[msg("Missing account in remaining_accounts")]     MissingAccount,
    #[msg("Only the authority can call this")]          Unauthorized,
    #[msg("Arithmetic overflow")]                       Overflow,
    #[msg("24-hour emergency timelock has not expired yet")] TimelockNotExpired,
    #[msg("Caller is not a deposited player in this game")]  NotAPlayer,
    #[msg("Token mint does not match this escrow")]     WrongMint,
}