# Open Auction Program
Enables simple decentralized auctions, which accepts bids and transfers the winning bid to the pre-specified beneficiary. Built on the Solana network.

## How does it work?
- Anyone can initialize an auction with a specified begin and end time
- Anyone can bid in the auction within the specified times. Auctions currently accept the SOL token
- After the auction end time passes, anyone can trigger the transfer of the winning bid to the beneficiary. All unsuccessful bidders can also receive a refund.

## Next Steps
- Allow the prize of the auction to be an SPL token or NFT, held by the auction program
- Support bidding via any SPL token
