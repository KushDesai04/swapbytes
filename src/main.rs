mod behaviour;
mod util;
use util::{ Cli, get_and_save_nickname, ChatState };
use behaviour::{ create_swapbytes_behaviour, handle_chat_event, handle_kademlia_event, ChatBehaviourEvent, SwapBytesBehaviourEvent };

use clap::Parser;
use futures::StreamExt;
use libp2p::{ gossipsub, kad, mdns, noise, request_response, swarm::SwarmEvent, tcp, yamux };
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
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Chat(chat_event)) => {
                    handle_chat_event(chat_event, &mut state, &mut swarm).await;
                },

                // Handle all Kademlia events
                SwarmEvent::Behaviour(SwapBytesBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {id, result, .. })) => {
                    handle_kademlia_event(id, result, &mut state).await;
                }

                // TODO: Handle all file exchange events

                _ => {}
        }
    }
    }
}
