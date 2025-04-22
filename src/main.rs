mod behaviour;
use behaviour::create_swapbytes_behaviour;

use clap::Parser;
use std::{error::Error, time::Duration};
use libp2p::{
    gossipsub, kad::Mode, noise, tcp, yamux, Multiaddr
};
use tokio::io::{self, AsyncBufReadExt};
use serde::{Deserialize, Serialize};

// File Exchange Protocol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRequest(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResponse(Vec<u8>);

/**
 *
 * Command-line options for configuring the libp2p application at start-up.
 *
 * Defines the available options:
 * - port: Specifies the port on which the application will run.
 * - peer: Defines a specific peer's multiaddress to connect to at start-up.
 */
#[derive(Parser, Debug)]
#[clap(name = "libp2p request response")]
pub struct Cli {
    #[arg(long)]
    port: Option<String>,

    #[arg(long)]
    peer: Option<Multiaddr>,
}

/**
 * Main function which controls the SwapBytes application.
 */
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    let cli = Cli::parse();

    // Generates the swarm used to connect and communicate with peers
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            create_swapbytes_behaviour(key)
                .expect("Failed to create combined behaviour")
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();


    // Creates a chatroom to be used by all connected peers by default
    let topic = gossipsub::IdentTopic::new("default");

    swarm.behaviour_mut().chat.gossipsub.subscribe(&topic)?;
    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    // Configures the peer to listen for incoming connection on tcp and udp over quic
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

    // Sets up a buffered reader to handle input from stdin
    let stdin = io::BufReader::new(io::stdin()).lines();

    let listen_port = cli.port.unwrap_or("0".to_string());
    let multiaddr = format!("/ip4/0.0.0.0/tcp/{listen_port}");
    let _ = swarm.listen_on(multiaddr.parse()?)?;

    if let Some(peer) = cli.peer {
        swarm.dial(peer).unwrap();
    }

    Ok(())
}

