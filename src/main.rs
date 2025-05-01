mod behaviour;
mod util;
mod input;

use futures::StreamExt;
use util::{ Cli, get_and_save_nickname, ChatState };
use input::handle_input;
use behaviour::{create_swapbytes_behaviour, handle_chat_event, handle_kademlia_event, handle_req_res_event, RendezvousBehaviourEvent, RequestResponseBehaviourEvent, SwapBytesBehaviourEvent};
use clap::Parser;
use libp2p::{ gossipsub, kad, multiaddr::Protocol, noise, rendezvous, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId };
use std::{ collections::HashMap, error::Error, time::Duration };
use tokio::{io::{ self, AsyncBufReadExt }, select, time::MissedTickBehavior};

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
        pending_connections: HashMap::new(),
        pending_rating_update: HashMap::new(),
        rendezvous: "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
                .parse::<PeerId>()
                .unwrap(),
    };

    // Creates a chatroom to be used by all connected peers by default
    let mut topic = gossipsub::IdentTopic::new("default");

    swarm.behaviour_mut().chat.gossipsub.subscribe(&topic)?;
    swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));

    // Rendezvous server
    let rendezvous_addr = cli.peer.unwrap_or("127.0.0.1".to_string());
    let rendezvous_point_address = format!("/ip4/{}/tcp/62649", rendezvous_addr)
        .parse::<Multiaddr>()
        .unwrap();

    let external_address = format!("/ip4/{}/tcp/0", rendezvous_addr)
        .parse::<Multiaddr>()
        .unwrap();
    swarm.add_external_address(external_address);
    swarm.dial(rendezvous_point_address.clone()).unwrap();

    // Discovery ping goes off every 30 seconds
    let mut discover_tick = tokio::time::interval(Duration::from_secs(30));
    discover_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Configures the peer to listen for incoming connection on tcp and udp over quic
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

    // Sets up a buffered reader to handle input from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let listen_port = cli.port.unwrap_or("0".to_string());
    let multiaddr = format!("/ip4/0.0.0.0/tcp/{listen_port}");
    let _ = swarm.listen_on(multiaddr.parse()?)?;

    let peer_id = *swarm.local_peer_id();
    let nickname = get_and_save_nickname(&mut stdin, peer_id, &mut swarm).await;

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                handle_input(line.trim(), &mut swarm, &mut topic, &mut state, nickname.clone(), &mut stdin).await;
            },

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Your node is listening on {}", address);
                },

                // Handle all chat events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Chat(chat_event)) => {
                    handle_chat_event(chat_event, &mut state, &mut swarm).await;
                },

                // Handle all Kademlia events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {id, result, .. })) => {
                    handle_kademlia_event(id, result, &mut state, &mut swarm).await;
                },


                // Handle all file exchange events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::RequestResponse(RequestResponseBehaviourEvent::RequestResponse(request_response_event))) => {
                    handle_req_res_event(request_response_event, &mut swarm, &mut stdin, &mut topic).await;
                },

                SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == state.rendezvous => {
                    if let Err(error) = swarm.behaviour_mut().rendezvous.rendezvous.register(
                        rendezvous::Namespace::from_static("rendezvous"),
                        state.rendezvous,
                        None,
                    ) {
                        println!("Failed to register: {error}");
                    } else {
                        println!("Connection established with rendezvous point {}", peer_id);
                        swarm.behaviour_mut().rendezvous.rendezvous.discover(
                            Some(rendezvous::Namespace::new("rendezvous".to_string()).unwrap()),
                            None,
                            None,
                            state.rendezvous,
                        )
                    }
                },
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Rendezvous(RendezvousBehaviourEvent::Rendezvous(rendezvous::client::Event::Discovered {
                    registrations,
                    ..
                }))) => {
                    for registration in registrations {
                        for address in registration.record.addresses() {
                            let peer = registration.record.peer_id();
                            println!("Discovered peer: {} at address: {}", peer, address);

                            let p2p_suffix = Protocol::P2p(peer);
                            let address_with_p2p =
                                if !address.ends_with(&Multiaddr::empty().with(p2p_suffix.clone())) {
                                    address.clone().with(p2p_suffix)
                                } else {
                                    address.clone()
                                };

                            swarm.dial(address_with_p2p).unwrap();
                        }
                    }
                }

                _ => {},
            },


            // If discovery tick, try to discover new peers
            _ = discover_tick.tick() => {
                swarm.dial(rendezvous_point_address.clone()).unwrap();
                swarm.behaviour_mut().rendezvous.rendezvous.discover(
                    Some(rendezvous::Namespace::new("rendezvous".to_string()).unwrap()),
                    None,
                    None,
                    state.rendezvous
                )
        },

        }
    }
}

