use bincode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};
use serde::{Deserialize, Serialize};

const ALPN: &[u8] = b"iroh-example/echo/0";
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
enum Message {
    Echo,
    Ping,
    Pong,
}

// ====================
// Unidirectional Stream Solution
// ====================

/// Send one message on a new unidirectional stream
async fn send_one_way(conn: &Connection, msg: &Message) -> Result<()> {
    let mut send = conn.open_uni().await.anyerr()?;

    let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;

    Ok(())
}

/// Receive one message from a unidirectional stream
async fn recv_one_way(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
    let bytes = recv.read_to_end(MAX_MESSAGE_SIZE).await.anyerr()?;

    let (msg, _) = bincode::decode_from_slice(&bytes, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(msg)
}

// ====================
// Application Logic
// ====================

#[tokio::main]
async fn main() -> Result<()> {
    run_singleplayer().await?;
    Ok(())
}

async fn run_server_internal() -> Result<Router> {
    let endpoint = Endpoint::bind().await?;
    let router = Router::builder(endpoint).accept(ALPN, Echo).spawn();
    println!("Server started at {:#?}", router.endpoint().addr());
    Ok(router)
}

async fn run_client_internal(addr: EndpointAddr) -> Result<()> {
    let endpoint = Endpoint::bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    // Infinite stress test: send a message every 100ms
    let mut message_count = 0u64;
    let messages = vec![Message::Echo, Message::Ping, Message::Pong];

    loop {
        let msg = &messages[message_count as usize % messages.len()];

        match send_one_way(&conn, msg).await {
            Ok(_) => {
                message_count += 1;
                if message_count % 10 == 0 {
                    println!("Sent {} messages", message_count);
                }
            }
            Err(e) => {
                eprintln!("Error sending message: {}", e);
                break;
            }
        }

        //tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    Ok(())
}

async fn run_singleplayer() -> Result<()> {
    let router = run_server_internal().await?;
    router.endpoint().online().await;
    let server_addr = router.endpoint().addr();

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Run client (will run infinitely)
    run_client_internal(server_addr).await?;

    Ok(())
}

#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("Accepted connection from {}", endpoint_id);

        let mut receive_count = 0u64;

        // Accept unidirectional streams in a loop
        loop {
            match connection.accept_uni().await {
                Ok(recv) => {
                    // Spawn a task to handle each stream independently
                    tokio::spawn(async move {
                        match recv_one_way(recv).await {
                            Ok(msg) => {
                                // Just log occasionally to avoid spam
                                if receive_count % 10 == 0 {
                                    println!(
                                        "Server received message #{}: {:?}",
                                        receive_count, msg
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    });

                    receive_count += 1;
                }
                Err(_) => {
                    // Connection closed
                    println!("Connection closed after {} messages", receive_count);
                    break;
                }
            }
        }

        Ok(())
    }
}
