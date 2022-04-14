# Prferential Consensus Peer-to-Peer

This simple program provides means to reach pereferential consensus through a peer-to-peer network. All the votes are mapped to memory and also held in a Kademlia key-value storage.

## How to use:

The program requires at least one quoroum to put the records into the Kademlia KV Storage. So you should start a peer server by `cargo run` then copy the address and open another terminal and `cargo run <addr>`

After that use one of the terminals to `INIT <votename> <bar>` a vote, (vote name should be there but it's currently not important) for example `INIT myvote 3` this means exactly 3 people should vote using three peers.

Then launch the necessary number of peers and `VOTE <name> <yay-preference> <nay-preference>`. Name should be unique. For either pereferences you can choose from

1. VeryHigh = 1.0
2. High = 0.75
3. Medium = 0.5
4. Low = 0.25
5. VeryLow = 0

After you're done voting simply pass `TALLY` to see if the consensus has been reached. It basically adds up pereference weights.