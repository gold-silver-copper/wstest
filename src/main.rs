use iroh::{
    Endpoint, EndpointAddr,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};

const ALPN: &[u8] = b"iroh-example/echo/0";

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

    // Send message
    let msg = b"Hello from client";
    send.write_all(msg).await.anyerr()?;
    send.finish().anyerr()?;
    println!("Sent: {}", String::from_utf8_lossy(msg));

    // Receive response
    let response = recv.read_to_end(1024).await.anyerr()?;
    println!("Received: {}", String::from_utf8_lossy(&response));

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

        let bytes_sent = tokio::io::copy(&mut recv, &mut send).await?;
        println!("Echoed {} byte(s)", bytes_sent);

        send.finish()?;
        connection.closed().await;

        Ok(())
    }
}
