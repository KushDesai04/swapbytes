# SwapBytes P2P file-sharing app

Swapbytes is a peer-to-peer file sharing app with chat, direct message, and file trade features.

## Features
- Decentralized chat using Gossipsub
- File share logic
- Private DMs for file trading and messagins
- Peer discovery using mDNS and Kademlia
- Rendezvous server support
- Rating system to see peer ratings


## Building
1. If you haven't already, [install Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)
2. Clone repository (git clone https://github.com/KushDesai04/swapbytes)
3. `cd swapbytes` to navigate to the swapbytes folder
4. Run `cargo build`
5. For each peer, run `cargo run --` (see command-line options for configuration)


## Getting started
### Setting up rendezvous server
If a server is already running, you can simply connect to it with the Command-Line options in the next section.
Otherwise, you can run a server locally by:
1. Cloning the libp2p rendezvous server example (https://github.com/libp2p/rust-libp2p.git)
2. `cd rust-libp2p/examples/rendezvous`
3. Run `cargo run --bin rendezvous-example`

### Command-line options
- `--port <port>`: Port number to listen on, defaults to a random unused port
- `--peer <ip>`: An optional rendezvous server address, defaults to the local network.

### IMPORTANT: If a rendezvous server is not found, the application will run using mDNS for peer discovery.

For example:
```bash
cargo run -- --port 9999 --rendezvous 10.0.0.1
```

### Enter your nickname
When the app starts up, you will be asked for a nickname to identify yourself.

### Commands
Any multiword arguments should be wrapped in double quotes. For example:
```bash
/connect "kush desai"
```
Commands are case-insensitive, but arguments are case-sensitive.
#### General Commands
- `/help`: Show a help message.
- `/list`: List all the peers currently on the network.
- `/connect <nickname>`: Request a private chat with another peer. You will be put into a private chat if the other peer accepts.
- `/exit`: Quit out of SwapBytes
- `<message>`: Send a message

#### Commands when in a private chat
- `/help`: Show a help message.
- `/list`: List all the peers currently on the network.
- `/offer <filename>`: Offer a user a file.
- `/request <filename>`: Request a file from a user.
- `/leave`: Leave a private chat. This will prompt you to rate the other peer before you connect back to the general chat room.
- `/exit`: Quit out of SwapBytes
- `<message>`: Send a message
