use std::str::FromStr;

use libp2p::{gossipsub::{self, TopicHash}, kad};
use tokio::io;

use crate::{behaviour::SwapBytesBehaviour, util::{update_peer_rating, ChatState, ConnectionRequest}};

pub async fn handle_input(line: &str, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>, topic : &mut gossipsub::IdentTopic, state: &mut ChatState, own_nickname: String, stdin: &mut io::Lines<io::BufReader<io::Stdin>>) {
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
                // Send NicknameLookup request to the peer with your own nickname
                state.pending_connections.insert(query_id, ConnectionRequest::NicknameLookup(own_nickname.clone(), swarm.local_peer_id().clone()));
            } else {
                println!("Usage: /connect <peer nickname>");
            }
        },

        "/leave" => {
            // get the other peer's nickname that is connected to the current topic


            let topic_hash: TopicHash = topic.hash().clone();
            if topic_hash.as_str() != "default" {
                //split the topic hash to get the other peer's nickname
                let parts: Vec<&str> = topic_hash.as_str().split('-').collect();
                let nickname1 = parts[0].to_string();
                let nickname2 = parts[1].to_string();
                let other_peer_nickname;
                let other_peer_id;
                if nickname1 == own_nickname {
                    other_peer_nickname = nickname2;
                    other_peer_id = parts[2];
                } else {
                    other_peer_nickname = nickname1;
                    other_peer_id = parts[3];
                }
                // send a leave message to the other peer
                println!("Please rate {} before leaving the chatroom: -1, 0, 1", other_peer_nickname);
                loop {
                    match stdin.next_line().await {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                let rating = trimmed.to_string();
                                if rating == "-1" || rating == "0" || rating == "1" {
                                    // update the rating of the other peer in the Kademlia routing table
                                    if let Ok(parsed_rating) = rating.parse::<i32>() {
                                        if let Ok(peer_id) = libp2p::PeerId::from_str(other_peer_id) {
                                            update_peer_rating(swarm, peer_id, parsed_rating, state).await;
                                        } else {
                                            println!("Failed to parse PeerId from the given string.");
                                        }
                                        println!("You have left the chatroom and rated {} with {}", other_peer_id, rating);
                                        break;
                                    } else {
                                        println!("Failed to parse rating. Please enter a valid number.");
                                    }
                                } else {
                                println!("Please enter a valid rating: -1, 0, 1");
                            }
                            } else {
                                println!("Rating cannot be empty. Please enter a valid rating.");
                            }
                        }

                        Ok(None) => {
                            println!("No input received. Please try again.");
                        }
                        Err(_) => {
                            println!("Error reading input. Please try again.");
                        }
                    }
                }
                let default_topic = gossipsub::IdentTopic::new("default");
                swarm.behaviour_mut().chat.gossipsub.unsubscribe(topic);
                swarm.behaviour_mut().chat.gossipsub.subscribe(&default_topic).unwrap();
                *topic = default_topic;
            } else {
                println!("You are already in the default chatroom.");
            }
        },

        "/list" => {
            // list all the connected peers
            let peers = swarm.connected_peers().collect::<Vec<_>>();
            println!("Connected peers: {:?}", peers);
        },
        "/kademliaList" => {
            // list all the connected peers in the Kademlia routing table
            let peers = swarm.behaviour_mut().kademlia.kbuckets();
            let peers_vec: Vec<_> = peers.map(|bucket| {
                bucket.iter().map(|entry| entry.node.key.clone()).collect::<Vec<_>>()
            }).collect::<Vec<_>>();
            println!("Connected peers in Kademlia routing table: {:?}", peers_vec);
        },

        _ => {
            // Periodic querying from existing peers
            let record_key = kad::RecordKey::new(&swarm.local_peer_id().to_bytes());
            swarm.behaviour_mut().kademlia.get_record(record_key);
            if let Err(e) = swarm.behaviour_mut().chat.gossipsub.publish(topic.clone(), line.as_bytes()) {
                println!("Publishing error: {:?}", e);
            }
        }
    }
}