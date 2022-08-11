use anchor_lang::{
    prelude::*, solana_program::program::invoke, solana_program::system_instruction,
};

// TODO: Replace with keypair from target after devnet deploy
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const LAMPORTS_BUFFER: u64 = 20_000_000;

#[program]
pub mod auction {
    use super::*;

    /// Creates and initialize a new auction
    pub fn initialize_auction(
        ctx: Context<InitializeAuction>,
        bidding_start_time: i64,
        bidding_end_time: i64,
    ) -> Result<()> {
        let auction_state = &mut ctx.accounts.auction_state;
        let clock = &ctx.accounts.clock;

        //Check that start time is >= current time and that end time is after the start time
        if bidding_start_time < clock.unix_timestamp {
            msg!("Start time invalid");
            return Err(AuctionError::StartTimeTooEarly.into());
        };

        if bidding_end_time <= bidding_start_time {
            msg!("End time invalid");
            return Err(AuctionError::EndingTimeTooEarly.into());
        };

        auction_state.initializer = *ctx.accounts.initializer.key;
        auction_state.bidding_start_time = bidding_start_time;
        auction_state.bidding_end_time = bidding_end_time;

        auction_state.beneficiary = *ctx.accounts.beneficiary.key;
        auction_state.highest_bid_address = None;
        auction_state.highest_bid_amount = None;
        auction_state.ended_funds_transferred = false;
        auction_state.bump = *ctx.bumps.get("auction_state").unwrap();

        Ok(())
    }

    /// Bid
    #[access_control(valid_bid_time(&ctx))]
    pub fn bid(ctx: Context<MakeBid>, amount: u64) -> Result<()> {
        let auction_state = &mut ctx.accounts.auction_state;
        let highest_bid = auction_state.highest_bid_amount;

        // Check if bid is higher than the highest bid
        if highest_bid.is_some() && amount <= highest_bid.unwrap() {
            return Err(AuctionError::BidTooLow.into());
        };

        // Transfer lamports from signer/ bidder to bid account
        let bidder = &mut ctx.accounts.bidder;
        let bid_account = &mut ctx.accounts.bid_account;
        let lamports_in_bid_account = bid_account.to_account_info().lamports();
        let total_lamports_needed = amount.checked_add(LAMPORTS_BUFFER).unwrap(); // TODO: Better error handling here

        // Check if amount in bid account is enough, if not, then transfer additional lamports from the bidder address to the bid account
        if lamports_in_bid_account < total_lamports_needed {
            let transfer_amount = total_lamports_needed
                .checked_sub(lamports_in_bid_account)
                .unwrap();

            let transfer_from_bidder_instruction =
                system_instruction::transfer(bidder.key, &bid_account.key(), transfer_amount);
            let account_infos = [bidder.to_account_info(), bid_account.to_account_info()];

            invoke(&transfer_from_bidder_instruction, &account_infos)?;
        }

        // Assert bid_account has enough lamports to fulfill the bid
        let lamports_in_bid_account = bid_account.to_account_info().lamports();
        assert!(lamports_in_bid_account >= amount.checked_add(LAMPORTS_BUFFER).unwrap());

        // Write the highest bid address and amount to auction state account
        auction_state.highest_bid_address = Some(*bidder.as_ref().key);
        auction_state.highest_bid_amount = Some(amount);

        // Write data to bid PDA
        bid_account.bidder = *bidder.key;
        bid_account.amount = amount;
        bid_account.auction = auction_state.key();
        bid_account.bump = *ctx.bumps.get("bid_account").unwrap();

        Ok(())
    }

    /// After an auction ends (determined by `bidding_end_time`), anyone can initiate end_auction
    /// which will transfer the highest bid from the bid account to the beneficiary listed in the auction state account
    #[access_control(end_auction_time_valid(&ctx.accounts.auction_state, &ctx.accounts.clock))]
    pub fn end_auction(ctx: Context<EndAuction>) -> Result<()> {
        let auction_state = &mut ctx.accounts.auction_state;
        let bid_account = &mut ctx.accounts.bid_account;
        let beneficiary = &mut ctx.accounts.beneficiary;

        if auction_state.ended_funds_transferred {
            return Err(AuctionError::AuctionAlreadyEnded.into());
        };

        // Check that the bid_account PDA corresponds to the auction_state PDA, even though I think this is actually redundant
        if auction_state.key() != bid_account.auction {
            return Err(AuctionError::AccountMismatch.into());
        }

        // Check that the correct beneficiary account was passed in. Also potentially redundant with anchor constraint below
        if *beneficiary.key != auction_state.beneficiary {
            return Err(AuctionError::InvalidBeneficiary.into());
        };

        // Check that the bidder in the bid account PDA matches the highest bid address in the auction state PDA
        let highest_bid_address = auction_state
            .highest_bid_address
            .ok_or(AuctionError::NoBids)?;
        if bid_account.bidder != highest_bid_address {
            return Err(AuctionError::AccountMismatch.into());
        };

        // Decrease lamports in bid_account PDA and increase beneficiary lamports by the bid amount
        let transfer_amount = auction_state
            .highest_bid_amount
            .ok_or(AuctionError::NoBids)?;
        msg!("Begin first transfer");

        let bid_account_info = bid_account.to_account_info();
        **bid_account_info.try_borrow_mut_lamports()? = bid_account_info
            .lamports()
            .checked_sub(transfer_amount)
            .unwrap();
        **beneficiary.try_borrow_mut_lamports()? =
            beneficiary.lamports().checked_add(transfer_amount).unwrap();

        msg!("First transfer complete");

        auction_state.ended_funds_transferred = true;

        //Close bid_account PDA
        let bid_account_lamports = bid_account_info.lamports();
        **bid_account_info.try_borrow_mut_lamports()? = bid_account_lamports
            .checked_sub(bid_account_lamports)
            .unwrap();
        let bidder_account_info = &mut ctx.accounts.bidder.to_account_info();
        **bidder_account_info.try_borrow_mut_lamports()? = bidder_account_info
            .lamports()
            .checked_add(bid_account_lamports)
            .unwrap();

        //TODO: Change to transfer closed balance to protocol wallet / treasury

        Ok(())
    }

    /// After an auction ends (the initializer/seller already received the winning bid),
    /// the unsuccessfull bidders can claim their money back by calling this instruction
    #[access_control(end_auction_time_valid(&ctx.accounts.auction_state, &ctx.accounts.clock))]
    pub fn refund(ctx: Context<RefundBid>) -> Result<()> {
        let auction_state = &ctx.accounts.auction_state;
        let bid_account = &mut ctx.accounts.bid_account;

        // Check that the auction ended field of the auction state account is true, signaling winning bid has been processed
        if !auction_state.ended_funds_transferred {
            return Err(AuctionError::InvalidRefund.into());
        };

        // Given a bid PDA, make sure its not the same as the highest bidder address and amount on the auction state account
        if bid_account.bidder == auction_state.highest_bid_address.unwrap()
            || bid_account.amount == auction_state.highest_bid_amount.unwrap()
        {
            return Err(AuctionError::HighestBidderCannotRefund.into());
        }

        // "Transfer" bid PDA amount to bidder and close account
        // (note: not a CPI to system program because system program does not own the PDA)
        let bid_account_info = bid_account.to_account_info();
        let bidder = &mut ctx.accounts.bidder;
        **bidder.try_borrow_mut_lamports()? = bidder
            .lamports()
            .checked_add(bid_account_info.lamports())
            .unwrap();
        **bid_account_info.try_borrow_mut_lamports()? = 0;

        Ok(())
    }
}

#[account]
pub struct AuctionState {
    initializer: Pubkey,
    bidding_start_time: i64,
    bidding_end_time: i64,
    beneficiary: Pubkey,
    highest_bid_address: Option<Pubkey>,
    highest_bid_amount: Option<u64>,
    ended_funds_transferred: bool,
    bump: u8,
}

#[derive(Accounts)]
pub struct InitializeAuction<'info> {
    #[account(mut)]
    pub initializer: Signer<'info>,
    #[account(
        init,
        payer = initializer,
        space = 8 + 32 + 8 + 8 + 32 + 33 + 9 + 1 + 1, //Note: Leading 8 is for the discriminant
        seeds = [
            b"auction-state",
            initializer.key().as_ref()
        ],
        bump
    )]
    pub auction_state: Account<'info, AuctionState>,
    /// CHECK: TBD
    pub beneficiary: AccountInfo<'info>,
    pub clock: Sysvar<'info, Clock>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Bid {
    bidder: Pubkey,
    amount: u64,
    auction: Pubkey,
    bump: u8,
}

#[derive(Accounts)]
pub struct MakeBid<'info> {
    #[account(mut)]
    pub bidder: Signer<'info>,
    #[account(
        init_if_needed, // Question for Ackee team: anchor docs warn about "re-initialization attacks". Is there a vulnerability here?
        payer = bidder,
        space = 8 + 32 + 8 + 32 + 1, //Note: Leading 8 is for the discriminant
        seeds = [
            b"bid",
            bidder.key().as_ref(),
            auction_state.key().as_ref()
        ],
        bump
    )]
    pub bid_account: Account<'info, Bid>,
    #[account(
        mut,
        seeds = [
            b"auction-state",
            auction_state.initializer.as_ref()
        ],
        bump = auction_state.bump
    )]
    pub auction_state: Account<'info, AuctionState>,
    pub clock: Sysvar<'info, Clock>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EndAuction<'info> {
    #[account(
        mut,
        seeds = [
            b"auction-state",
            auction_state.initializer.as_ref()
        ],
        bump = auction_state.bump
    )]
    pub auction_state: Account<'info, AuctionState>,
    #[account(
        mut,
        seeds = [
            b"bid",
            bid_account.bidder.as_ref(),
            auction_state.key().as_ref(), //Note: This should ensure that the bid_account PDA corresponds to the auction_state PDA above, right?
        ],
        bump = bid_account.bump
    )]
    pub bid_account: Account<'info, Bid>,
    #[account(mut, constraint = *bidder.key == bid_account.bidder)]
    /// CHECK: performed above but getting error during anchor build
    pub bidder: AccountInfo<'info>,
    #[account(mut, constraint = *beneficiary.key == auction_state.beneficiary)]
    /// CHECK: performed above but getting error during anchor build
    pub beneficiary: AccountInfo<'info>,
    pub clock: Sysvar<'info, Clock>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RefundBid<'info> {
    #[account(
        mut,
        seeds = [
            b"auction-state",
            auction_state.initializer.as_ref()
        ],
        bump = auction_state.bump
    )]
    pub auction_state: Account<'info, AuctionState>,
    #[account(
        mut,
        seeds = [
            b"bid",
            bid_account.bidder.as_ref(),
            auction_state.key().as_ref(),
        ],
        bump = bid_account.bump
    )]
    pub bid_account: Account<'info, Bid>,
    #[account(mut, constraint = *bidder.key == bid_account.bidder)]
    // Check that the bidder fields of bid_account matches the signer
    pub bidder: Signer<'info>,
    pub clock: Sysvar<'info, Clock>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum AuctionError {
    #[msg("Start Time must be greater than or equal to the current time")]
    StartTimeTooEarly,
    #[msg("End Time must be greater than the Start Time")]
    EndingTimeTooEarly,
    #[msg("Bid must be greater than the current highest bid")]
    BidTooLow,
    #[msg("Auction has already ended and funds have been transferred")]
    AuctionAlreadyEnded,
    #[msg("Bid account does not correspond to the correct auction account")]
    AccountMismatch,
    #[msg("Auction had no bids")]
    NoBids,
    #[msg("Beneficiary account provided does not match the auction state account's beneficiary")]
    InvalidBeneficiary,
    #[msg("Bidder on Bid Account does not match highest bidder on auction account")]
    IncorrectBidAccount,
    #[msg("Cannot refund bid prior to auction end and settling of winning bid")]
    InvalidRefund,
    #[msg("The highest bid in the auction cannot be refunded")]
    HighestBidderCannotRefund,
    #[msg("Bids can only be submitted after the auction has begun")]
    BidTooEarly,
    #[msg("Bids can only be submitted before the auction ends")]
    BidTooLate,
    #[msg("Cannot end auction before auction end time elapses")]
    AuctionNotOver,
}

fn valid_bid_time(ctx: &Context<MakeBid>) -> Result<()> {
    let auction_state = &ctx.accounts.auction_state;
    let clock = &ctx.accounts.clock;

    if auction_state.bidding_start_time > clock.unix_timestamp {
        return Err(AuctionError::BidTooEarly.into());
    }

    if clock.unix_timestamp > auction_state.bidding_end_time {
        return Err(AuctionError::BidTooLate.into());
    }

    Ok(())
}

fn end_auction_time_valid(
    auction_state: &Account<AuctionState>,
    clock: &Sysvar<Clock>,
) -> Result<()> {
    if auction_state.bidding_end_time > clock.unix_timestamp {
        return Err(AuctionError::AuctionNotOver.into());
    }

    Ok(())
}
