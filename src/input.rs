use libp2p::{gossipsub, kad};

use crate::{behaviour::SwapBytesBehaviour, util::{ChatState, ConnectionRequest}};

pub async fn handle_input(line: &str, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>, topic : &mut gossipsub::IdentTopic, state: &mut ChatState) {
    match line {
        "/exit" => {
            println!("Thank you for using SwapBytes! Goodbye!");
            std::process::exit(0);
        },
        "/help" => {
            println!("Available commands: /exit, /help, /list, /connect <peer nickname>, <message>");
        },
        "/list" => {
            println!("Connected peers: {:?}", swarm.connected_peers().collect::<Vec<_>>());
        },

        // /connect <peer>
        val if val.starts_with("/connect") => {
            let parts: Vec<&str> = val.split_whitespace().collect();
            if parts.len() == 2 {
                let peer_nickname = parts[1].to_string();
                let reverse_key = kad::RecordKey::new(&format!("nickname:{}", peer_nickname));
                let query_id = swarm.behaviour_mut().kademlia.get_record(reverse_key);
                state.pending_connections.insert(query_id, ConnectionRequest::NicknameLookup);
            } else {
                println!("Usage: /connect <peer nickname>");
            }
        },

        _ => {
            if let Err(e) = swarm.behaviour_mut().chat.gossipsub.publish(topic.clone(), line.as_bytes()) {
                println!("Publish error: {:?}", e);
            }
        }
    }
}