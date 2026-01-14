use iridium_stomp::{Connection, Frame};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This example expects a STOMP broker on localhost:61613 (e.g. RabbitMQ with stomp plugin).
    // Start a local broker before running: `docker compose up -d` from the repo root.

    let conn = Connection::connect("127.0.0.1:61613", "guest", "guest", "10000,10000").await?;

    let msg = Frame::new("SEND")
        .header("destination", "/queue/test")
        .set_body(b"hello from iridium-stomp example".to_vec());

    conn.send_frame(msg).await?;

    // Try to read one incoming frame (if any), but don't block forever — time out after 5s.
    match tokio::time::timeout(Duration::from_secs(5), conn.next_frame()).await {
        Ok(Some(frame)) => println!("received frame:\n{}", frame),
        Ok(None) => println!("connection closed, no frames received"),
        Err(_) => println!("timed out waiting for a frame"),
    }

    // Close the connection (ensure the background task is signalled to shut down)
    conn.close().await;

    Ok(())
}
