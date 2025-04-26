use libp2p::{gossipsub::{self, TopicHash}, kad};

use crate::{behaviour::SwapBytesBehaviour, util::{ChatState, ConnectionRequest}};

pub async fn handle_input(line: &str, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>, topic : &mut gossipsub::IdentTopic, state: &mut ChatState) {
    match line {
        "/exit" => {
            println!("Thank you for using SwapBytes! Goodbye!");
            std::process::exit(0);
        },
        "/help" => {
            let topic_hash: TopicHash = topic.hash().clone();
            if topic_hash.as_str() == "default" {
                println!("Available commands:\n
                /help - display a list of available commands\n
                /exit - leave SwapBytes\n
                /connect <peer nickname>\n
                <message>");
            } else {
                println!("Available commands:\n
                /help - display a list of available commands\n
                /exit - leave SwapBytes\n
                /connect <peer nickname> - invite a peer to a private room\n
                /leave - leave the current chatroom\n
                <message>");
            }

        },

        // this is for /connect <peer>
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

        "/leave" => {
            println!("You have left the chatroom.");
            let topic_hash: TopicHash = topic.hash().clone();
            if topic_hash.as_str() != "default" {
                let default_topic = gossipsub::IdentTopic::new("default");
                swarm.behaviour_mut().chat.gossipsub.unsubscribe(topic);
                swarm.behaviour_mut().chat.gossipsub.subscribe(&default_topic).unwrap();
                *topic = default_topic;
            } else {
                println!("You are already in the default chatroom.");
            }
        },

        _ => {
            if let Err(e) = swarm.behaviour_mut().chat.gossipsub.publish(topic.clone(), line.as_bytes()) {
                println!("Publishing error: {:?}", e);
            }
        }
    }
}