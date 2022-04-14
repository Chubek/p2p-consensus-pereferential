
use async_std::{io, task};
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
    record::Key, AddProviderOk, Kademlia, 
    KademliaEvent, PeerRecord,
    PutRecordOk, QueryResult, Quorum,Record,
};
use libp2p::{
    development_transport, identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    swarm::{NetworkBehaviourEventProcess, SwarmEvent},
    NetworkBehaviour, PeerId, Swarm,
};
use std::error::Error;

#[async_std::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let transport = development_transport(local_key).await?; 

    #[derive(NetworkBehaviour)]
    #[behaviour(event_process = true)]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: Mdns,
        
        #[behaviour(ignore)]
        tally: Tally,
    }

    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        fn inject_event(&mut self, event: MdnsEvent) {
            if let MdnsEvent::Discovered(list) = event {
                for (peer_id, multiaddr) in list {
                    self.kademlia.add_address(&peer_id, multiaddr);  
                }
            }
        }
    }

    impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
        fn inject_event(&mut self, message: KademliaEvent) {
            println!("Event took place");
            match message {
                KademliaEvent::OutboundQueryCompleted { result, .. } => match result {
                    QueryResult::GetProviders(Ok(ok)) => {
                        for peer in ok.providers {
                            println!(
                                "Peer {:?} provides key {:?}",
                                peer,
                                std::str::from_utf8(ok.key.as_ref()).unwrap()
                            );
                        }
                    }
                    QueryResult::GetProviders(Err(err)) => {
                        eprintln!("Failed to get providers: {:?}", err);
                    }
                    QueryResult::GetRecord(Ok(ok)) => {
                        for PeerRecord {
                            record: Record { key, value, .. },
                            ..
                        } in ok.records
                        {
                            
                            let k = std::str::from_utf8(key.as_ref()).unwrap();
                            let v = std::str::from_utf8(&value).unwrap();

                            if k.contains("INIT") {
                                self.tally = self.tally.clone().set_bar(v.parse::<u8>().unwrap())
                            } else if k.contains("VOTE") {
                                let v_split = v.split("-").collect::<Vec<&str>>();
                                self.tally = self.tally.clone().push_new(&mut self.kademlia, k, 
                                    v_split[0], v_split[1]);
                            } else if k.contains("TALLY") {
                                let res = self.tally.clone().tally_up();

                                println!("{:?}", res);
                            }
                        }
                    }
                    QueryResult::GetRecord(Err(err)) => {
                        eprintln!("Failed to get record: {:?}", err);
                    }
                    QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                        println!(
                            "Successfully put record {:?}",
                            std::str::from_utf8(key.as_ref()).unwrap()
                        );
                    }
                    QueryResult::PutRecord(Err(err)) => {
                        eprintln!("Failed to put record: {:?}", err);
                    }
                    QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                        println!(
                            "Successfully put provider record {:?}",
                            std::str::from_utf8(key.as_ref()).unwrap()
                        );
                    }
                    QueryResult::StartProviding(Err(err)) => {
                        eprintln!("Failed to put provider record: {:?}", err);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }


    let mut swarm = {
        let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = task::block_on(Mdns::new(MdnsConfig::default()))?;
        let mut tally = Tally::new();
        let behaviour = MyBehaviour { kademlia, mdns, tally};
        Swarm::new(transport, behaviour, local_peer_id)
    };


    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse(); 

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;


    loop {
        select! {
            line = stdin.select_next_some() => handle_input_line(
                &mut swarm.behaviour_mut().kademlia,
                line.expect("Stdin failed")
            ),
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr {address, ..} => {
                    println!("Listening in {:?}", address)
                },
                _ => {}
            },

        }
    }
}

#[derive(Clone)]
enum Preference {
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Err,
}

#[allow(dead_code)]
#[derive(Clone)]
struct Vote {
    pub yay: Preference,
    pub nay: Preference,
    pub key: Key,
}


impl Preference {
    fn new(pref: &str) -> Self {
        match pref.to_lowercase().as_str() {
            "veryhigh" => Self::VeryHigh,
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            "verylow" => Self::VeryLow,
            _ => Self::Err,
        }
    }
}

impl From<Preference> for f32 {
    fn from(pref: Preference) -> Self {
        match pref {
            Preference::VeryHigh => 1.0f32,
            Preference::High => 0.75f32,
            Preference::Medium => 0.5f32,
            Preference::Low => 0.25f32,
            Preference::VeryLow => 0.0f32,
            Preference::Err => -1.0f32,
        }
    }
}

impl Vote {
    pub fn new(key: Key, yay_pref: &str, nay_pref: &str) -> Self {
        let yay = Preference::new(yay_pref);
        let nay = Preference::new(nay_pref); 

        Vote {yay, nay, key}
    }

    fn yay_nay_repr(self) -> String {
        let yay_f32: f32 = self.yay.into();
        let nay_f32: f32 = self.nay.into();


        format!("{}-{}", yay_f32, nay_f32)
    }

    pub fn register(self, kademlia: &mut Kademlia<MemoryStore>) -> Self {
        let key = self.clone().key;
        let value = self.clone().yay_nay_repr().as_bytes().to_vec();
        
        let record = Record {
            key,
            value,
            publisher: None,
            expires: None,
        };
        kademlia
            .put_record(record, Quorum::One)
            .expect("Failed to store record locally.");

            self
        }

}

#[allow(unused_mut)]
#[allow(dead_code)]
#[derive(Clone)]
struct Tally {
    pub keys: Vec<String>,
    pub votes: Vec<Vote>,
    pub bar: u8,
}

#[derive(Debug)]
enum Consensus {
    YayReached,
    NayReached,
    Impass,
    BarNotMet,
}

impl Consensus {
    pub fn new(cons_yay: f32, cons_nay: f32) -> Self {
        if cons_nay > cons_yay {
            return Self::NayReached
        } else if cons_nay < cons_yay {
            return Self::YayReached
        } 

        Self::Impass
    }
}

#[derive(Debug)]
struct TallyResult {
    yays_consensus: f32,
    nays_consensus: f32,
    result: Consensus,
}


impl TallyResult {
    pub fn new(yays_consensus: f32, nays_consensus: f32, bar_met: bool) -> Self {
        if !bar_met {
            return TallyResult {yays_consensus, nays_consensus, result: Consensus::BarNotMet}
        }

        let result = Consensus::new(yays_consensus, nays_consensus);

        TallyResult { yays_consensus, nays_consensus, result}
    }
}

impl Tally {
    pub fn new() -> Self {
        let tallies: Vec<Vote> = Vec::new();
        let keys: Vec<String> = Vec::new();

        Tally { votes: tallies, keys, bar: 0u8 }
    }

    pub fn set_bar(mut self, bar: u8) -> Self {
        self.bar = bar;

        self
    }

    pub fn push_new(mut self, 
                kademlia: &mut Kademlia<MemoryStore>, 
                key_str: &str, yay_pref: &str, nay_pref: &str) -> Self {
        let key = Key::new(&key_str);
        
        let new_vote = Vote::new(key, yay_pref, nay_pref); 

        self.votes.push(new_vote.register(kademlia));
        self.keys.push(key_str.to_string());

        self

    }

    pub fn tally_up(self) -> TallyResult {
        let mut bar_met = true;
        
        if self.votes.len() < self.bar as usize {
            bar_met = false;
        }      
        
        let mut yays_consensus = 0f32;
        let mut nays_consensus = 0f32;

        for vote in self.votes {
            let f32_yay: f32 = vote.yay.into();
            let f32_nay: f32  = vote.nay.into();

            yays_consensus += f32_yay;
            nays_consensus += f32_nay;
        }

        let res = TallyResult::new(yays_consensus, nays_consensus, bar_met);

        res

    }

    
}



fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) {
    let mut args = line.split(' ');

    match args.next() {
        Some("INIT") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&format!("{}-{}", "INIT", key)),
                    None => {
                        eprintln!("Expected vote name");
                        return;
                    }
                }
            };
            let bar = {
                match args.next() {
                    Some(bar) => bar.as_bytes().to_vec(),
                    None => {
                        eprintln!("Expected bar");
                        return;
                    }
                }
            };
            let record = Record {
                key: key.clone(),
                value: bar,
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
            kademlia.get_record(key, Quorum::One);
        }
        Some("VOTE") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&format!("{}-{}", "VOTE", key)),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let yay_pref = {
                match args.next() {
                    Some(yay_pref) => yay_pref,
                    None => {
                        eprintln!("Expected yay preference");
                        return;
                    }
                }
            };
            let nay_pref = {
                match args.next() {
                    Some(nay_pref) => nay_pref,
                    None => {
                        eprintln!("Expected nay preference");
                        return;
                    }
                }
            };

            let record = Record {
                key: key.clone(),
                value: format!("{}-{}", yay_pref, nay_pref).as_bytes().to_vec(),
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
            kademlia.get_record(key, Quorum::One);
           
           
        }
        Some("TALLY") => {
            let key = Key::new(&"TALLY");

            let record = Record {
                key: key.clone(),
                value: format!("").as_bytes().to_vec(),
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
            kademlia.get_record(key, Quorum::One);

        }
        _ => {
            eprintln!("expected INIT, VOTE, TALLY");
        }
    }
}