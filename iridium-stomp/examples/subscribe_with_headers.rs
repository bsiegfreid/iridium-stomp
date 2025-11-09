use futures::StreamExt;
use iridium_stomp::Connection;
use iridium_stomp::connection::AckMode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect("127.0.0.1:61613", "guest", "guest", "10000,10000").await?;

    // Example: pass broker-specific subscribe headers. For RabbitMQ durable
    // queues you typically create the durable queue server-side and then
    // subscribe to the named queue; for ActiveMQ you might pass a
    // subscriptionName header. Here we show a generic header example.
    let headers = vec![("example-header".to_string(), "value".to_string())];

    let mut subscription = conn
        .subscribe_with_headers("/queue/example", AckMode::Client, headers)
        .await?;

    while let Some(frame) = subscription.next().await {
        println!("received frame:\n{}", frame);
        // ack if desired
    }

    Ok(())
}
