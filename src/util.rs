use std::collections::HashMap;
use clap::Parser;
use libp2p::{ kad, Multiaddr, PeerId };
use serde::{Deserialize, Serialize};
use tokio::io;

use crate::behaviour::SwapBytesBehaviour;

#[derive(Parser, Debug)]
#[clap(name = "libp2p request response")]
pub struct Cli {
    #[arg(long)]
    pub port: Option<String>,

    #[arg(long)]
    pub peer: Option<Multiaddr>,
}
pub enum ConnectionRequest {
    NicknameLookup,
    PeerData(PeerId),
}
pub struct ChatState {
    pub pending_messages: HashMap<kad::QueryId, (PeerId, Vec<u8>)>,
    pub pending_connections: HashMap<kad::QueryId, ConnectionRequest>,
}

#[derive(Serialize, Deserialize)]
pub struct PeerData {
    pub nickname: String,
    pub rating: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {  // New struct for the invite data
    pub room_id: String,
    pub initiator_nickname: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivateRoomProtocol {
    Invite(Invite),
    Accept(String),
    Reject(String),
}

pub async fn get_and_save_nickname(
    stdin: &mut io::Lines<io::BufReader<io::Stdin>>,
    peer_id: Vec<u8>,
    swarm: &mut libp2p::Swarm<SwapBytesBehaviour>
) -> String{
    let nickname;
    println!("Enter a nickname: ");
    loop {
        match stdin.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    nickname = trimmed.to_string();
                    break;
                } else {
                    println!("Nickname cannot be empty. Please enter a valid nickname.");
                }
            }
            Ok(None) => {
                println!("No input received. Please try again.");
            }
            Err(e) => {
                println!("Error reading input: {}. Please try again.", e);
            }
        }
    }

    println!("Your nickname is: {}", nickname);
    let peer_data = PeerData {
        nickname: nickname.trim().to_string(),
        rating: 0, // Initial rating
    };

    let serialized = serde_json::to_vec(&peer_data).expect("Serialization failed");

    let nickname_record = kad::Record {
        key: kad::RecordKey::new(&peer_id),
        value: serialized,
        publisher: None,
        expires: None,
    };

    swarm
        .behaviour_mut()
        .kademlia.put_record(nickname_record, kad::Quorum::One)
        .expect("Failed to store record locally.");

    // Storing nickname: peer record - uses double the storage but allows for easy lookup
    let reverse_key = kad::RecordKey::new(
        &format!("nickname:{}", nickname).as_bytes()
    );
    let reverse_record = kad::Record {
        key: reverse_key,
        value: peer_id,
        publisher: None,
        expires: None,
    };
    swarm
        .behaviour_mut()
        .kademlia.put_record(reverse_record, kad::Quorum::One)
        .expect("Failed to store reverse record locally.");
    nickname
}
