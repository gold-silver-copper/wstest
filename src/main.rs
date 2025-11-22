use bincode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};
use serde::{Deserialize, Serialize};

const ALPN: &[u8] = b"iroh-example/echo/0";

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
enum Message {
    Echo,
}

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
    let (mut send, mut recv) = conn.open_bi().await.anyerr()?;

    // Serialize and send message
    let msg = Message::Echo;
    let encoded = bincode::encode_to_vec(&msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;
    println!("Sent: {:?}", msg);

    // Receive and deserialize response
    let response_bytes = recv.read_to_end(1024).await.anyerr()?;
    let (response, _): (Message, usize) =
        bincode::decode_from_slice(&response_bytes, bincode::config::standard())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    println!("Received: {:?}", response);

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
        let (mut send, mut recv) = connection.accept_bi().await?;

        // Read encoded message
        let encoded_msg = recv.read_to_end(1024).await.expect("READ TO END ERROR");

        // Decode the message (ignore errors)
        let (msg, _): (Message, usize) =
            match bincode::decode_from_slice(&encoded_msg, bincode::config::standard()) {
                Ok(result) => result,
                Err(_e) => {
                    println!("Failed to decode message, using default Echo");
                    (Message::Echo, 0)
                }
            };
        println!("Received: {:?}", msg);

        // Encode and send back the same message (ignore errors)
        let encoded_response = match bincode::encode_to_vec(&msg, bincode::config::standard()) {
            Ok(encoded) => encoded,
            Err(_e) => {
                println!("Failed to encode message");
                vec![]
            }
        };

        send.write_all(&encoded_response)
            .await
            .expect("CANNOT WRITE ALL");
        println!("Sent echo: {:?}", msg);
        send.finish()?;

        connection.closed().await;
        Ok(())
    }
}
