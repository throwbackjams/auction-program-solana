import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Auction } from "../target/types/auction";

describe("auction", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.Provider.env()
  anchor.setProvider(provider);

  const program = anchor.workspace.Auction as Program<Auction>;
  const initializer = anchor.web3.Keypair.generate();
  const beneficiary = anchor.web3.Keypair.generate();
  const bidder_one = anchor.web3.Keypair.generate();
  const bidder_two = anchor.web3.Keypair.generate();
  console.log("initializer pubkey: ", initializer.publicKey.toBase58());
  console.log("bidder_one pubkey: ", bidder_one.publicKey.toBase58());
  console.log("bidder_two pubkey: ", bidder_two.publicKey.toBase58());
  console.log("beneficiary pubkey: ", beneficiary.publicKey.toBase58());
  
  const bid_one_amount = new anchor.BN(900_000_000);
  console.log("bid_one amount: ", bid_one_amount.toNumber());
  const bid_two_amount = new anchor.BN(1_000_000_000);
  console.log("bid_two amount: ", bid_two_amount.toNumber());

  const currentTime = new anchor.BN(Date.now() / 1000);
  const biddingStartTime = currentTime;
  const biddingEndTime = currentTime.add(new anchor.BN(5));

  function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  it("Initializes Auction!", async () => {
    
    const signature = await provider.connection.requestAirdrop(initializer.publicKey, 1_000_000_000)
    await provider.connection.confirmTransaction(signature, 'confirmed')
    
    const [auctionStatePDA, ] = await PublicKey 
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("auction-state"),
          initializer.publicKey.toBuffer()
        ],
        program.programId
      );
      
    await program.methods
      .initializeAuction(biddingStartTime, biddingEndTime)
      .accounts({
        initializer: initializer.publicKey,
        auctionState: auctionStatePDA,
        beneficiary: beneficiary.publicKey,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
    })
    .signers([initializer])
    .rpc();

    const auctionState = await program.account.auctionState.fetch(auctionStatePDA)
    
    expect(auctionState.initializer.toBase58()).to.equal(initializer.publicKey.toBase58());
    expect(auctionState.biddingStartTime.cmp(biddingStartTime)).to.equal(0);
    expect(auctionState.biddingEndTime.cmp(biddingEndTime)).to.equal(0);
    expect(auctionState.beneficiary.toBase58()).to.equal(beneficiary.publicKey.toBase58());
    expect(auctionState.highestBidAddress).to.equal(null);
    expect(auctionState.highestBidAmount).to.equal(null);
    expect(auctionState.endedFundsTransferred).to.equal(false);
    });
    
    it("Make first bid", async() => {
      // Step 1: Get auction state PDA    
      const [auctionStatePDA, ] = await PublicKey
        .findProgramAddress(
          [
            anchor.utils.bytes.utf8.encode("auction-state"),
            initializer.publicKey.toBuffer()
          ],
          program.programId
        );

    // Step 2: Make a bid
    const signature = await provider.connection.requestAirdrop(bidder_one.publicKey, 2_000_000_000)
    await provider.connection.confirmTransaction(signature, 'confirmed')
    
    const initialBalance = await provider.connection.getBalance(bidder_one.publicKey)
    console.log('bidder_one initial balance: ', initialBalance)

    await provider.connection.confirmTransaction(signature, 'confirmed')

    const [bidOnePDA, ] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("bid"),
          bidder_one.publicKey.toBuffer(),
          auctionStatePDA.toBuffer(),
        ],
        program.programId
      );

    if (Date.now() < biddingStartTime.toNumber() * 1000) {
      console.log("too early")
      await sleep(biddingStartTime.toNumber() * 1000 - Date.now() + 1000);
    }
    
    await program.methods
      .bid(bid_one_amount)
      .accounts({
        bidder: bidder_one.publicKey,
        bidAccount: bidOnePDA,
        auctionState: auctionStatePDA,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      })
      .signers([bidder_one])
      .rpc();

      // Check the lamports of the bidder and bid account
      const bidAccount = await program.account.bid.fetch(bidOnePDA);
      expect(bidAccount.bidder.toBase58()).to.equal(bidder_one.publicKey.toBase58());
      expect(bidAccount.amount.toNumber()).to.equal(bid_one_amount.toNumber());
      expect(bidAccount.auction.toBase58()).to.equal(auctionStatePDA.toBase58());
      const balance = await provider.connection.getBalance(bidder_one.publicKey);
      console.log('after bid balance: ', balance);
      expect(balance).to.be.lt(initialBalance - bid_one_amount.toNumber());

      // Check that auction state recorded new highest bidder
      const auctionState = await program.account.auctionState.fetch(auctionStatePDA);
      expect(auctionState.highestBidAddress.toBase58()).to.equal(bidder_one.publicKey.toBase58());
      expect(auctionState.highestBidAmount.toNumber()).to.equal(bid_one_amount.toNumber());

    })

    it("Make higher bid", async() => {
      // Step 1: Get auction state PDA    
      const [auctionStatePDA, ] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("auction-state"),
          initializer.publicKey.toBuffer()
        ],
        program.programId
      );

      // Step 2: Make a bid
      const signature = await provider.connection.requestAirdrop(bidder_two.publicKey, 2_000_000_000)
      await provider.connection.confirmTransaction(signature, 'confirmed')
      
      const initialBalance = await provider.connection.getBalance(bidder_two.publicKey)
      console.log('bidder_two initial balance: ', initialBalance)

      await provider.connection.confirmTransaction(signature, 'confirmed')

      const [bidTwoPDA, ] = await PublicKey
        .findProgramAddress(
          [
            anchor.utils.bytes.utf8.encode("bid"),
            bidder_two.publicKey.toBuffer(),
            auctionStatePDA.toBuffer(),
          ],
          program.programId
        );
      
      await program.methods
        .bid(bid_two_amount)
        .accounts({
          bidder: bidder_two.publicKey,
          bidAccount: bidTwoPDA,
          auctionState: auctionStatePDA,
          clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        })
        .signers([bidder_two])
        .rpc();

        // Check the lamports of the bidder and bid account
        const bidAccount = await program.account.bid.fetch(bidTwoPDA);
        expect(bidAccount.bidder.toBase58()).to.equal(bidder_two.publicKey.toBase58());
        expect(bidAccount.amount.toNumber()).to.equal(bid_two_amount.toNumber());
        expect(bidAccount.auction.toBase58()).to.equal(auctionStatePDA.toBase58());
        const balance = await provider.connection.getBalance(bidder_two.publicKey);
        console.log('bidder_two after bid balance: ', balance);
        expect(balance).to.be.lt(initialBalance - bid_two_amount.toNumber());

        // Check that auction state recorded new highest bidder
        const auctionState = await program.account.auctionState.fetch(auctionStatePDA);
        expect(auctionState.highestBidAddress.toBase58()).to.equal(bidder_two.publicKey.toBase58());
        expect(auctionState.highestBidAmount.toNumber()).to.equal(bid_two_amount.toNumber());      
    })

    it("End auction and transfer funds", async() => {
      // Wait until auction is over

      // Get auction state PDA and bid PDA
      const [auctionStatePDA, ] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("auction-state"),
          initializer.publicKey.toBuffer()
        ],
        program.programId
      );

      const [bidTwoPDA, ] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("bid"),
          bidder_two.publicKey.toBuffer(),
          auctionStatePDA.toBuffer(),
        ],
        program.programId
      );

      console.log("bid account PDA: ", bidTwoPDA.toBase58());
      console.log("auctionState account PDA: ", auctionStatePDA.toBase58());

      if (Date.now() < biddingEndTime.toNumber() * 1000) {
        console.log("cannot end auction yet")
        await sleep(biddingEndTime.toNumber() * 1000 - Date.now() + 2000);
      }

      // Send end auction instruction
      await program.methods
        .endAuction()
        .accounts({
          auctionState: auctionStatePDA,
          bidAccount: bidTwoPDA,
          bidder: bidder_two.publicKey,
          beneficiary: beneficiary.publicKey,
          clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        })
        .rpc();

      // Check that beneficiary sol balance received the auction end transfer
      const beneficiaryBalance = await provider.connection.getBalance(beneficiary.publicKey);
      console.log('beneficiary balance after auction end: ', beneficiaryBalance);
      expect(beneficiaryBalance).to.be.gte(bid_two_amount.toNumber());
      
      // Check bid account is closed / lamports is zero
      program.account.bid.fetch(bidTwoPDA)
        .then(() => {
          console.log("Closed BID PDA found. Program did not behave as expected")
          expect(1).to.equal(2);
        })
        .catch(err => expect(err).to.contain("Error: Account does not exist"))

      // Check auction state account field ended is true
      const auctionState = await program.account.auctionState.fetch(auctionStatePDA);
      expect(auctionState.endedFundsTransferred).to.equal(true)

    })

    it("Refund first bid", async() => {
      const [auctionStatePDA, ] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("auction-state"),
          initializer.publicKey.toBuffer()
        ],
        program.programId
      );
  
      const [bidOnePDA, ] = await PublicKey
        .findProgramAddress(
          [
            anchor.utils.bytes.utf8.encode("bid"),
            bidder_one.publicKey.toBuffer(),
            auctionStatePDA.toBuffer(),
          ],
          program.programId
        );
        
      const bidderOneAfterBidBalance = await provider.connection.getBalance(bidder_one.publicKey);

      if (Date.now() < biddingEndTime.toNumber() * 1000) {
        console.log("cannot end auction yet")
        await sleep(biddingEndTime.toNumber() * 1000 - Date.now() + 1000);
      }

      await program.methods
        .refund()
        .accounts({
          bidAccount: bidOnePDA,
          bidder: bidder_one.publicKey,
          auctionState: auctionStatePDA,
          clock:anchor.web3.SYSVAR_CLOCK_PUBKEY,
        })
        .signers([bidder_one])
        .rpc();

      // Check that bidder_one sol balance increased
      const bidderOneFinalBalance = await provider.connection.getBalance(bidder_one.publicKey);
      console.log('bidderOne balance after auction end: ', bidderOneFinalBalance);
      expect(bidderOneFinalBalance).to.be.gte(bidderOneAfterBidBalance + bid_one_amount.toNumber());

      // Check that the bid account is closed
      program.account.bid.fetch(bidOnePDA)
      .then(() => {
        console.log("Closed BID PDA found. Program did not behave as expected")
        expect(1).to.equal(2);
      })
      .catch(err => expect(err).to.contain("Error: Account does not exist"))
    })
});
