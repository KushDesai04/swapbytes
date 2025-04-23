mod behaviour;
mod util;
use util::{ Cli, get_and_save_nickname, ChatState };
use behaviour::{ create_swapbytes_behaviour, ChatBehaviourEvent, SwapBytesBehaviourEvent };

use clap::Parser;
use futures::StreamExt;
use libp2p::{ gossipsub, kad, mdns, noise, swarm::SwarmEvent, tcp, yamux };
use std::{ collections::HashMap, error::Error, time::Duration };
use tokio::{ io::{ self, AsyncBufReadExt }, select };

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Generates the swarm used to connect and communicate with peers
    let mut swarm = libp2p::SwarmBuilder
        ::with_new_identity()
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic()
        .with_behaviour(|key| {
            create_swapbytes_behaviour(key).expect("Failed to create combined behaviour")
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let mut state = ChatState {
        pending_messages: HashMap::new(),
    };

    // Creates a chatroom to be used by all connected peers by default
    let topic = gossipsub::IdentTopic::new("default");

    swarm.behaviour_mut().chat.gossipsub.subscribe(&topic)?;
    swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));

    // Configures the peer to listen for incoming connection on tcp and udp over quic
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

    // Sets up a buffered reader to handle input from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let listen_port = cli.port.unwrap_or("0".to_string());
    let multiaddr = format!("/ip4/0.0.0.0/tcp/{listen_port}");
    let _ = swarm.listen_on(multiaddr.parse()?)?;

    if let Some(peer) = cli.peer {
        swarm.dial(peer).unwrap();
    }

    let peer_id = swarm.local_peer_id().to_string().clone();
    get_and_save_nickname(&mut stdin, &peer_id, &mut swarm).await;

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm.behaviour_mut().chat.gossipsub.publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {:?}", e);
                };
            }

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => println!("Your node is listening on {address}"),

                // Handle all chat events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Chat(chat_event)) => match chat_event {
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
                    let key = kad::RecordKey::new(&peer_id.to_string());
                    let query_id = swarm.behaviour_mut().kademlia.get_record(key);

                    // Store message data and query ID for later processing
                    let message_data = message.data.clone();
                    state.pending_messages.insert(query_id, (peer_id.clone(), message_data));

                },
                _ => {}
                },
                // Handle all Kademlia events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {id, result, .. })) => {
                    match result {
                        kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record))) => {
                            if let Some((peer_id, msg)) = state.pending_messages.remove(&id) {
                                // Triggered when an incoming message is recieved and the nickname is found
                                let kad::PeerRecord { record: kad::Record { value, .. }, .. } = peer_record;
                                if let Ok(nickname) = std::str::from_utf8(&value) {
                                    println!("{nickname}: {}", String::from_utf8_lossy(&msg));
                                } else {
                                    println!("Peer {peer_id}: {}", String::from_utf8_lossy(&msg));
                                }
                            }
                        },

                        kad::QueryResult::GetRecord(Err(kad::GetRecordError::NotFound { .. })) => {
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
                _ => {}
        }
    }
    }
}
