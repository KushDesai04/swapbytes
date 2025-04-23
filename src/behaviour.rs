use serde::{Deserialize, Serialize};
use libp2p::{
    gossipsub, kad::{self, store::MemoryStore, QueryId, QueryResult}, mdns, request_response::{self, ProtocolSupport}, swarm::{self, NetworkBehaviour}, StreamProtocol
};

use crate::util::ChatState;

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
            let key = kad::RecordKey::new(&peer_id.to_string());
            let query_id = swarm.behaviour_mut().kademlia.get_record(key);

            // Store message data and query ID for later processing
            let message_data = message.data.clone();
            state.pending_messages.insert(query_id, (peer_id.clone(), message_data));

        },

        _ => {}
    }
}
pub async fn handle_kademlia_event(id: QueryId, result: QueryResult, state: &mut ChatState ) {
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