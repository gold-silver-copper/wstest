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
// Simple Solution: One Stream Per Message
// ====================

/// Send one message on a new stream
async fn send_message(conn: &Connection, msg: &Message) -> Result<()> {
    let (mut send, _recv) = conn.open_bi().await.anyerr()?;

    let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;

    Ok(())
}

/// Receive one message from a stream
async fn recv_message(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
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

    // Send multiple messages - each on its own stream!
    println!("Sending multiple messages...");
    send_message(&conn, &Message::Echo).await?;
    send_message(&conn, &Message::Ping).await?;
    send_message(&conn, &Message::Pong).await?;
    println!("Sent 3 messages");

    // Receive responses - each comes on its own stream
    for i in 0..3 {
        let (_send, recv) = conn.accept_bi().await.anyerr()?;
        let response = recv_message(recv).await?;
        println!("Received message {}: {:?}", i + 1, response);
    }

    conn.close(0u32.into(), b"bye!");
    endpoint.close().await;
    Ok(())
}

async fn run_singleplayer() -> Result<()> {
    let router = run_server_internal().await?;
    router.endpoint().online().await;
    let server_addr = router.endpoint().addr();

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Run client
    run_client_internal(server_addr).await?;

    println!("Singleplayer test complete!");
    router.shutdown().await.anyerr()?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("Accepted connection from {}", endpoint_id);

        // Accept multiple streams in a loop
        loop {
            match connection.accept_bi().await {
                Ok((_send, recv)) => {
                    // Spawn a task to handle each stream independently
                    let conn = connection.clone();
                    tokio::spawn(async move {
                        match recv_message(recv).await {
                            Ok(msg) => {
                                println!("Server received: {:?}", msg);

                                // Echo back on a NEW stream
                                if let Err(e) = send_message(&conn, &msg).await {
                                    eprintln!("Error sending response: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    });
                }
                Err(_) => {
                    // Connection closed
                    println!("Connection closed");
                    break;
                }
            }
        }

        Ok(())
    }
}

// ====================
// Bonus: Even Simpler Unidirectional Version
// ====================

// For send-only messages (no response needed)
#[allow(dead_code)]
async fn send_one_way(conn: &Connection, msg: &Message) -> Result<()> {
    let mut send = conn.open_uni().await.anyerr()?;

    let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;

    Ok(())
}

// For receiving one-way messages
#[allow(dead_code)]
async fn recv_one_way(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
    let bytes = recv.read_to_end(MAX_MESSAGE_SIZE).await.anyerr()?;

    let (msg, _) = bincode::decode_from_slice(&bytes, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(msg)
}
