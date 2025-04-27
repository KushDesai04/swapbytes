use serde::{Deserialize, Serialize};
use libp2p::{
    gossipsub::{self, IdentTopic}, kad::{self, store::MemoryStore, QueryId, QueryResult}, mdns, request_response::{self, ProtocolSupport}, swarm::NetworkBehaviour, PeerId, StreamProtocol
};
use tokio::{fs::File, io::{self, AsyncReadExt, AsyncWriteExt}};
use uuid::Uuid;
use crate::util::{ChatState, ConnectionRequest, Invite, PeerData, PrivateRoomProtocol};


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseType {
    FileResponse(Vec<u8>, String),
    PrivateRoomResponse(PrivateRoomProtocol),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestType {
    FileRequest(String, PeerId),
    PrivateRoomRequest(Invite),
}

#[derive(NetworkBehaviour)]
pub struct ChatBehaviour {
    pub mdns: mdns::tokio::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
}

#[derive(NetworkBehaviour)]
pub struct RequestResponseBehaviour {
    pub request_response: request_response::cbor::Behaviour<RequestType, ResponseType>,
}

#[derive(NetworkBehaviour)]
pub struct SwapBytesBehaviour {
    pub chat: ChatBehaviour,
    pub request_response: RequestResponseBehaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
}


pub fn create_swapbytes_behaviour(key: &libp2p::identity::Keypair) -> Result<SwapBytesBehaviour, Box<dyn std::error::Error>> {
    let chat_behaviour = ChatBehaviour {
        mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?,
        gossipsub: gossipsub::Behaviour::new(gossipsub::MessageAuthenticity::Signed(key.clone()), gossipsub::Config::default())?,
    };

    let request_response_behaviour = RequestResponseBehaviour {
        request_response: request_response::cbor::Behaviour::new([(
            StreamProtocol::new("/file-exchange/1"),
            ProtocolSupport::Full,
        )], request_response::Config::default()),
    };

    let kademlia_behaviour = kad::Behaviour::new(
                                                    key.public().to_peer_id(),
                                                MemoryStore::new(key.public().to_peer_id()));

    Ok(SwapBytesBehaviour {
        chat: chat_behaviour,
        request_response: request_response_behaviour,
        kademlia: kademlia_behaviour,
    })
}

pub async fn handle_chat_event(chat_event: ChatBehaviourEvent, state: &mut ChatState, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>) {
    match chat_event {
        ChatBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
            for (peer_id, multiaddr) in list {
                println!("mDns discovered new peer: {peer_id}, listening on {multiaddr}");
                swarm.behaviour_mut().chat.gossipsub.add_explicit_peer(&peer_id);
                swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
            }
        }

        ChatBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
            for (peer_id, multiaddr) in list {
                println!("mDNS peer has expired: {peer_id}, listening on {multiaddr}");
                swarm.behaviour_mut().chat.gossipsub.remove_explicit_peer(&peer_id);
            }
        }

        ChatBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source: peer_id,
            message_id: _id,
            message,
        }) => {
            let key = kad::RecordKey::new(&peer_id.to_bytes());
            let query_id = swarm.behaviour_mut().kademlia.get_record(key);

            // Store message data and query ID for later processing
            let message_data = message.data.clone();
            state.pending_messages.insert(query_id, (peer_id.clone(), message_data));

        },

        _ => {}
    }
}

pub async fn handle_kademlia_event(id: QueryId, result: QueryResult, state: &mut ChatState, swarm: &mut libp2p::Swarm<SwapBytesBehaviour> ) {
    match result {
        kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record))) => {
            if let Some((peer_id, msg)) = state.pending_messages.remove(&id) {
                match serde_json::from_slice::<PeerData>(&peer_record.record.value) {
                    Ok(peer) => {
                        println!("{} ( {}★ ): {}",
                            peer.nickname,
                            peer.rating,
                            String::from_utf8_lossy(&msg)
                        );
                    }
                    Err(_) => {
                        println!("Peer {peer_id}: {}", String::from_utf8_lossy(&msg));
                    }
                }
            } else if let Some(request_type) = state.pending_connections.remove(&id) {
                match request_type {
                    ConnectionRequest::NicknameLookup(initiator_nickname, initiator_peer_id) => {
                        match PeerId::from_bytes(&peer_record.record.value) {
                            Ok(peer_id) => {
                                // Check if the peer ID is not the same as the local peer ID
                                if peer_id == *swarm.local_peer_id() {
                                    println!("You cannot connect to yourself.");
                                    return;
                                }
                                let peer_data_key = kad::RecordKey::new(&peer_id.to_bytes());
                                let data_query_id = swarm.behaviour_mut().kademlia.get_record(peer_data_key);
                                state.pending_connections.insert(data_query_id, ConnectionRequest::PeerData(peer_id, initiator_nickname, initiator_peer_id));
                            }
                            Err(e) => {
                                println!("Invalid Peer ID in record: {:?}\nRaw bytes: {:?}",
                                    e,
                                    peer_record.record.value
                                );
                            }
                        }
                    },
                    ConnectionRequest::PeerData(other_peer_id, initiator_nickname, initiator_peer_id) => {
                        match serde_json::from_slice::<PeerData>(&peer_record.record.value) {
                            Ok(peer) => {
                                let room_id = format!("{}-{}-{}-{}-{}",initiator_nickname.clone(), peer.nickname.clone(), initiator_peer_id, other_peer_id, Uuid::new_v4().to_string());
                                swarm.behaviour_mut().request_response.request_response.send_request(
                                    &other_peer_id,
                                    RequestType::PrivateRoomRequest(Invite {
                                        room_id: room_id.clone(),
                                        initiator_nickname: initiator_nickname.clone(),
                                    })
                                );
                                println!("Private room request sent to {}. You will automatically connect if they accept", peer.nickname);
                            }
                            Err(e) => println!("Invalid peer data for {}: {}", other_peer_id, e),
                        }
                    },
                }
            } else if let Some(rating) = state.pending_rating_update.remove(&id) {
                match serde_json::from_slice::<PeerData>(&peer_record.record.value) {
                    Ok(peer) => {
                        // Update the peer's rating in the local store
                        let updated_peer = PeerData {
                            nickname: peer.nickname.clone(),
                            rating: peer.rating + rating,
                        };
                        let serialized = serde_json::to_vec(&updated_peer).expect("Serialization failed");
                        let updated_record = kad::Record {
                            key: peer_record.record.key,
                            value: serialized,
                            publisher: None,
                            expires: None,
                        };
                        // Store the updated record in the DHT
                        swarm.behaviour_mut().kademlia.put_record(updated_record, kad::Quorum::All).expect("Failed to store updated record locally.");
                        println!("Updated rating for {}: {}★", peer.nickname, updated_peer.rating);
                    }
                    Err(_) => {
                        println!("Error retrieving peer data for rating update: {}", String::from_utf8_lossy(&peer_record.record.value));
                    }
                }
            }
        },

        kad::QueryResult::GetRecord(Err(kad::GetRecordError::NotFound { .. })) => {
            println!("No peer found with that nickname.");
            if let Some((peer_id, msg)) = state.pending_messages.remove(&id) {
                println!("Peer {peer_id}: {}", String::from_utf8_lossy(&msg));
            }
        },

        kad::QueryResult::GetRecord(Err(err)) => {
            println!("Error retrieving record: {err}");
            if let Some((peer_id, msg)) = state.pending_messages.remove(&id) {
                println!("Peer {peer_id}: {}", String::from_utf8_lossy(&msg));
            }
        },

        _ => {}
    }
}

pub async fn handle_req_res_event(request_response_event: request_response::Event<RequestType, ResponseType>, swarm: &mut libp2p::Swarm<SwapBytesBehaviour>, stdin: &mut io::Lines<io::BufReader<io::Stdin>>, topic: &mut gossipsub::IdentTopic) {
    match request_response_event {
        request_response::Event::Message {message, ..} => match message {
            request_response::Message::Request { request: RequestType::FileRequest(filename, _requested_peer_id), channel, .. } => {
                // A file request has been received
                println!("Received file request for: {}", filename);
                println!("Do you want to send the file? (y/n)");
                let response;
                loop {
                    match stdin.next_line().await {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if trimmed == "y" || trimmed == "n" {
                                response = trimmed.to_string();
                                break;
                            } else {
                                println!("Invalid input. Please enter 'y' or 'n'.");
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
                if response == "n" {
                    // Send a rejection response
                    swarm.behaviour_mut().request_response.request_response.send_response(channel, ResponseType::FileResponse(vec![], String::new())).unwrap();
                } else {
                    // If the user accepts, read the file and send it
                    match File::open(filename.clone()).await {
                        Ok(mut file) => {
                            let mut buffer = Vec::new();
                            // Read the file into a buffer
                            if let Err(e) = file.read_to_end(&mut buffer).await {
                                println!("Failed to read file: {:?}", e);
                            }
                            // Send the response to the file requester
                            swarm.behaviour_mut().request_response.request_response.send_response(channel, ResponseType::FileResponse(buffer, filename))
                            .expect("Failed to send file response");
                        }
                        // If the file doesn't exist send an empty vector
                        Err(_) => {
                            println!("File not found. Sending empty response.");
                            swarm.behaviour_mut().request_response.request_response.send_response(channel, ResponseType::FileResponse(vec![], String::new()))
                            .expect("Failed to send file response");
                        }
                    };
                }


            },

            request_response::Message::Request { request: RequestType::PrivateRoomRequest(Invite { room_id, initiator_nickname }), channel, .. } => {
                // Handle private room request
                println!("Received private room request from {initiator_nickname}");
                // Ask user to accept or reject the request
                println!("Do you accept the private room request? (y/n)");
                let response ;
                loop {
                    match stdin.next_line().await {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if trimmed == "y" || trimmed == "n" {
                                response = trimmed.to_string();
                                break;
                            } else {
                                println!("Invalid input. Please enter 'y' or 'n'.");
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
                let private_room_response;
                if response == "y" {
                    private_room_response = PrivateRoomProtocol::Accept(room_id.clone());
                    // Connect to the private room topic
                    // Unsubscribe from the default topic
                    let default_topic = gossipsub::IdentTopic::new("default"); // or your current topic name
                    swarm.behaviour_mut().chat.gossipsub.unsubscribe(&default_topic);
                    // Subscribe to the private room topic
                    let private_topic = IdentTopic::new(format!("{room_id}"));
                    swarm.behaviour_mut().chat.gossipsub.subscribe(&private_topic).unwrap();
                    *topic = private_topic.clone();
                    println!("You have joined the private room: {room_id}");
                } else {
                    private_room_response = PrivateRoomProtocol::Reject(room_id.clone());
                };
                // Send the response back to the requester
                swarm.behaviour_mut().request_response.request_response.send_response(channel, ResponseType::PrivateRoomResponse(private_room_response))
                .expect("Failed to send private room response");
            },

            request_response::Message::Response {response: ResponseType::FileResponse(file_data, filename), request_id } => {
                if file_data.is_empty() {
                    println!("File request was rejected or file not found.");
                    return;
                }
                println!("Received file {:?}", file_data);
                // Save the response to a file
                let filename = format!("received_file_{}_{}", filename, request_id);
                let mut file = File::create(filename).await.unwrap();
                if let Err(e) = file.write_all(&file_data).await {
                    println!("Failed to write file: {:?}", e);
                } else {
                    println!("File received and saved successfully.");
                }
            },

            request_response::Message::Response {response: ResponseType::PrivateRoomResponse(protocol), .. } => {
                if let PrivateRoomProtocol::Reject(_room_id) = protocol {
                    println!("Private room request rejected.");
                } else if let PrivateRoomProtocol::Accept(room_id) = protocol {
                    // Connect to the private room topic
                    // Unsubscribe from the default topic
                    let default_topic = gossipsub::IdentTopic::new("default"); // or your current topic name
                    swarm.behaviour_mut().chat.gossipsub.unsubscribe(&default_topic);
                    // Subscribe to the private room topic
                    let private_topic = IdentTopic::new(format!("{room_id}"));
                    swarm.behaviour_mut().chat.gossipsub.subscribe(&private_topic).unwrap();
                    *topic = private_topic.clone();
                    println!("You have joined the private room: {room_id}");
                }
            }
        },

        // outgoing request fails to be sent
        request_response::Event::OutboundFailure {request_id, error, .. } => {
            println!("Request {:?} failed to send: {:?}", request_id, error);
        },

        // incoming request fails to be processed
        request_response::Event::InboundFailure {peer, request_id, error, .. } => {
            println!("Request {:?} from peer {:?} failed to be read: {:?}", request_id, peer, error);
        },

        // outgoing response is successfully sent
        request_response::Event::ResponseSent { .. } => {
            // Dont send anything here
        },
    }
}