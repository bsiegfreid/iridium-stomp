use iridium_stomp::{Connection, SubscriptionOptions};
use iridium_stomp::connection::AckMode;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This example expects a STOMP broker on localhost:61613 (e.g. RabbitMQ with stomp plugin).
    // Start a local broker before running: `docker compose up -d` from the repo root.

    // Connect with a heartbeat request (client_out=10000ms, client_in=10000ms)
    let conn = Connection::connect("127.0.0.1:61613", "guest", "guest", "10000,10000").await?;

    // Subscribe to a queue using client ack mode (so we must ack messages).
    // Here we use `SubscriptionOptions` to optionally specify a durable
    // queue name (for brokers like RabbitMQ) or include broker-specific
    // headers (for example ActiveMQ's durable subscription headers).
    let opts = SubscriptionOptions {
        durable_queue: Some("/queue/example-durable".to_string()),
        headers: vec![],
    };

    let mut subscription = conn.subscribe_with_options("/exchange/topic", AckMode::Client, opts).await?;

    println!("subscribed id={} dest={}", subscription.id(), subscription.destination());

    // Use the Subscription as a Stream directly (we implemented Stream for Subscription)
    while let Some(frame) = subscription.next().await {
        println!("received frame:\n{}", frame);

        // read message-id header for ACK
        let mut msg_id: Option<String> = None;
        for (k, v) in &frame.headers {
            if k.to_lowercase() == "message-id" {
                msg_id = Some(v.clone());
                break;
            }
        }

        if let Some(id) = msg_id {
            // Acknowledge the message via the subscription convenience method
            subscription.ack(&id).await?;
            println!("acked message-id={}", id);
        }
    }

    // Explicitly unsubscribe when done (consumes the subscription)
    subscription.unsubscribe().await?;

    // close the connection
    conn.close().await;
    Ok(())
}
