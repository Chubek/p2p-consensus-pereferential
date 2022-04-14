# Perferential Consensus Peer-to-Peer

This simple program provides means to reach pereferential consensus through a peer-to-peer network. All the votes are mapped to memory and also held in a Kademlia key-value storage.

## How to use:

1. `cargo run` and then `INIT <bar>`, bar being number of max people you want to vote (max necessary).
2. Open up same number of terminals as the bar, and peer through to the IP given to you in the stdout of the first connect. Then `VOTE <preferential yay> <orererential nay>`
3. From the main terminal to `TALLY`.