use serde::{Deserialize, Serialize};
use libp2p::{
    gossipsub, kad::{self, store::MemoryStore}, mdns, request_response::{self, ProtocolSupport}, swarm::NetworkBehaviour, StreamProtocol
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRequest(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResponse(pub Vec<u8>);

#[derive(NetworkBehaviour)]
pub struct ChatBehaviour {
    pub mdns: mdns::tokio::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
}

#[derive(NetworkBehaviour)]
pub struct RequestResponseBehaviour {
    pub request_response: request_response::cbor::Behaviour<FileRequest, FileResponse>,
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

    let kademlia_behaviour = kad::Behaviour::new(key.public().to_peer_id(), MemoryStore::new(key.public().to_peer_id()));

    Ok(SwapBytesBehaviour {
        chat: chat_behaviour,
        request_response: request_response_behaviour,
        kademlia: kademlia_behaviour,
    })
}