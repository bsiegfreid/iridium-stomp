# Durable subscriptions and broker-specific headers

This document explains how to achieve "subscription survives disconnects" semantics
with different message brokers and how to use the `subscribe_with_headers`
API in this crate to pass broker-specific options.

## Background

STOMP is a small, portable protocol for messaging. It intentionally leaves some
broker features underspecified (for portability). Different brokers provide
features like durable subscriptions in broker-specific ways:

- ActiveMQ: supports durable topic subscriptions via `client-id` and
  subscription name headers. These are part of ActiveMQ's extension to STOMP.
- RabbitMQ: does not provide ActiveMQ-style durable topic subscriptions. Use a
  durable queue (a named, durable queue bound to an exchange) to achieve
  equivalent behavior: messages published while your consumer is offline are
  persisted in the durable queue and delivered when the client reconnects and
  re-subscribes to the same queue name.

## Library support: extra subscribe headers

To support broker-specific options and to make durable subscription patterns
work, this library provides `Connection::subscribe_with_headers(...)` which
accepts arbitrary headers to include in the `SUBSCRIBE` frame. These headers are
stored locally with the subscription and automatically re-sent if the library
needs to reconnect and re-subscribe on your behalf.

API sketch

- `subscribe_with_headers(destination: &str, ack: AckMode, extra_headers: Vec<(String,String)>)`
  - Returns a `Subscription` handle.
  - `extra_headers` are included in the initial `SUBSCRIBE` frame and will be
    re-sent on reconnect.
- `subscribe(destination, ack)` is a convenience wrapper when you don't need
  extra headers.

## Examples

- RabbitMQ durable queue:
  1. Create a durable queue and bind it to the exchange that receives messages.
     This can be done via the management UI or server-side configuration.
  2. In your client, `CONNECT` as usual and `subscribe` to the named queue:

```rust
let conn = Connection::connect("127.0.0.1:61613", "guest", "guest", "0,0").await?;
let subscription = conn.subscribe("/queue/my-durable-queue", AckMode::Client).await?;
```

When the client disconnects, messages published to the queue while the client
was offline will be retained by RabbitMQ and delivered when the client
reconnects and re-subscribes to the same queue name.

- Broker-specific durable header (ActiveMQ example):

```rust
// ActiveMQ example (broker-specific headers) - not portable
let headers = vec![("activemq.subscriptionName".to_string(), "my-durable".to_string())];
let subscription = conn.subscribe_with_headers("/topic/foo", AckMode::Client, headers).await?;
```

Because headers are persisted locally by the library, when the library
reconnects it will re-issue the same `SUBSCRIBE` with the `activemq.subscriptionName`
header and the broker will resume the durable subscription.

## Notes & gotchas

- Durable queues and persistent messages: for persistence across broker restarts
  you need both a durable queue and persistent messages (publisher marks
  messages as persistent).
- ACK mode: choose `client` or `client-individual` if you need message-level
  acknowledgement semantics. Unacknowledged messages may be requeued by the
  broker on disconnect depending on broker configuration.
- Portability: using broker-specific headers reduces portability between brokers.
  The `subscribe_with_headers` API is intentionally flexible so you can pass
  what your broker expects.

If you'd like, I can add runnable integration examples (docker-compose +
example) that demonstrate durable queue usage with RabbitMQ and show reconnect
+ resubscribe behavior.
