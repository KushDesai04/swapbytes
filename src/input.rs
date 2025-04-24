use libp2p::{gossipsub, kad, PeerId};

use crate::{behaviour::SwapBytesBehaviour, util::ChatState};

pub async fn handle_input(line: &str, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>, topic : &mut gossipsub::IdentTopic, state: &mut ChatState) {
    match line {
        "/exit" => {
            println!("Thank you for using SwapBytes! Goodbye!");
            std::process::exit(0);
        },
        "/help" => {
            println!("Available commands: /exit, /help, <message>");
        },
        "/list" => {
            println!("Connected peers: {:?}", swarm.connected_peers().collect::<Vec<_>>());
        },

        // /connect <peer>
        val if val.starts_with("/connect") => {
            let parts: Vec<&str> = val.split_whitespace().collect();
            if parts.len() == 2 {
                let peer_name = parts[1];
                let key = kad::RecordKey::new(&peer_name);
                let query_id = swarm.behaviour_mut().kademlia.get_record(key);

                state.pending_connections.insert(query_id, swarm.local_peer_id().clone());
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