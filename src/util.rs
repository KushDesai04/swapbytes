use std::collections::HashMap;
use clap::Parser;
use libp2p::{ kad, Multiaddr, PeerId };
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
pub struct ChatState {
    pub pending_messages: HashMap<kad::QueryId, (PeerId, Vec<u8>)>,
}

pub async fn get_and_save_nickname(
    stdin: &mut io::Lines<io::BufReader<io::Stdin>>,
    peer_id: &String,
    swarm: &mut libp2p::Swarm<SwapBytesBehaviour>
) {
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
    let nickname_record = kad::Record {
        key: kad::RecordKey::new(peer_id),
        value: nickname.as_bytes().to_vec(),
        publisher: None,
        expires: None,
    };

    swarm
        .behaviour_mut()
        .kademlia.put_record(nickname_record, kad::Quorum::One)
        .expect("Failed to store record locally.");
}
