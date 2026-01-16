Subscription options and durable subscriptions
=============================================

This document explains how to use `SubscriptionOptions` (the typed options
we added to `iridium-stomp`) to support durable/subscription-like behavior on
common brokers: RabbitMQ and ActiveMQ.

Background
----------

STOMP itself is a small, text-based protocol and does not mandate a single,
portable mechanism for broker-side durable/topic subscriptions. Different
brokers provide different mechanisms:

- RabbitMQ: use durable, named queues bound to exchanges. Messages published
  to an exchange and routed to a durable queue will persist while the
  consumer is offline (assuming messages are published as persistent).
- ActiveMQ: supports broker-side durable topic subscriptions which normally
  require a `client-id` on `CONNECT` plus a durable subscription name on
  `SUBSCRIBE`. The exact header names are broker-specific — consult the
  broker documentation.

`iridium-stomp` approach
------------------------

To remain protocol- and broker-agnostic while still making these features
practical, `iridium-stomp` provides two small, flexible helpers:

- `Connection::subscribe_with_headers(destination, ack, extra_headers)` —
  low-level convenience that forwards arbitrary header pairs on the
  `SUBSCRIBE` frame and persists those pairs with the local subscription
  bookkeeping so they are automatically re-sent after a reconnect.

- `SubscriptionOptions` — a typed, ergonomic wrapper that includes:
  - `headers: Vec<(String,String)>` — extra headers to include on `SUBSCRIBE`.
  - `durable_queue: Option<String>` — convenience to override the destination
    with a named queue (useful for RabbitMQ durable queues).

These options let users pass broker-specific headers to enable durable
subscriptions on brokers that support them, while the library handles
re-sending the same SUBSCRIBE on reconnect.

RabbitMQ (durable queues)
-------------------------

RabbitMQ does not implement ActiveMQ-style durable topic subscriptions. The
recommended pattern is:

1. Create a durable queue and bind it to an exchange with the desired
   routing key(s). This is an administrative operation you can do via the
   management UI, `rabbitmqadmin`, or the AMQP API. Example (rabbitmqadmin):

   ```sh
   rabbitmqadmin declare queue name=my-app-queue durable=true
   rabbitmqadmin declare binding source=amq.topic destination=my-app-queue routing_key="topic.#"
   ```

2. Ensure messages published to the exchange are persistent (broker-side
   durability). How to mark messages persistent depends on the publisher
   (AMQP clients have `delivery_mode`/`persistent` settings, STOMP
   publishers may include broker-specific headers). See RabbitMQ docs.

3. Subscribe to the named durable queue using `SubscriptionOptions.durable_queue`:

   ```rust
   use iridium_stomp::{Connection, SubscriptionOptions};
   use iridium_stomp::connection::AckMode;
   use tokio::runtime::Runtime;

   // create a small runtime for the example and unwrap results for simplicity
   let rt = Runtime::new().expect("create tokio runtime");
   rt.block_on(async {
     let conn = Connection::connect(
       "127.0.0.1:61613",
       "guest",
       "guest",
       Connection::DEFAULT_HEARTBEAT,
     ).await.unwrap();

     let opts = SubscriptionOptions {
       durable_queue: Some("/queue/my-app-queue".to_string()),
       headers: vec![],
     };

     let _subscription = conn
       .subscribe_with_options("/exchange/amq.topic", AckMode::Client, opts)
       .await
       .unwrap();

     // subscription now reads from the durable named queue. Messages that arrived
     // while the consumer was offline will be delivered when the client
     // re-subscribes to the same queue name.
   });
   ```

Notes:
- The library will persist any headers you set in `SubscriptionOptions.headers`
  and will resend them on reconnect, so re-subscribing to the same durable
  queue is automatic.
- The publisher must mark messages persistent if you need durability across
  broker restarts. The details are publisher- and broker-specific.

ActiveMQ (durable topic subscriptions)
--------------------------------------

ActiveMQ exposes a broker-side durable subscription mechanism for topics.
Typical steps are:

1. Establish a `CONNECT` frame that includes a broker `client-id` (this
   ties durable subscriptions to a client identity).
2. Issue a `SUBSCRIBE` frame with the durable subscription name (broker
   specific header such as `activemq.subscriptionName` or similar — check
   the broker docs for the exact header required).

In `iridium-stomp` you can pass the per-subscription durable header via
`SubscriptionOptions.headers` so the library will forward it on `SUBSCRIBE`
and persist it for resubscribe on reconnect.

Example:

   ```rust
   use iridium_stomp::{Connection, ConnectOptions, SubscriptionOptions};
   use iridium_stomp::connection::AckMode;
   use tokio::runtime::Runtime;

   // create a small runtime for the example and unwrap results for simplicity
   let rt = Runtime::new().expect("create tokio runtime");
   rt.block_on(async {
     // ActiveMQ requires a `client-id` on CONNECT to make subscriptions durable.
     // Use ConnectOptions to set the client-id:
     let connect_opts = ConnectOptions::new()
       .client_id("my-durable-client");

     let conn = Connection::connect_with_options(
       "activemq-host:61613",
       "user",
       "pass",
       Connection::NO_HEARTBEAT,
       connect_opts,
     ).await.unwrap();

     let mut headers = Vec::new();
     // Example header — consult ActiveMQ STOMP docs for the exact header name
     headers.push(("activemq.subscriptionName".to_string(), "my-durable-sub".to_string()));

     let opts = SubscriptionOptions {
       durable_queue: None,
       headers,
     };

     let _subscription = conn
       .subscribe_with_options("/topic/my-topic", AckMode::Client, opts)
       .await
       .unwrap();
   });
   ```

Important caveats:
- Header names for durable subscriptions are broker-specific — consult your
  broker's STOMP documentation.

Resubscribe behavior
--------------------

`iridium-stomp` persists the subscribe headers you supply in
`SubscriptionEntry` and automatically re-issues the same `SUBSCRIBE` frames
(if the connection is re-established). This is intended to make durable
subscriber workflows simpler: when you reconnect, the crate will attempt to
recreate your subscriptions with the same headers and ids.

Recommendations
---------------

- For RabbitMQ durability, prefer creating named durable queues bound to
  exchanges and subscribe to the named queue via
  `SubscriptionOptions.durable_queue`. Document the queue binding and
  persistent publishing policies in your app's deployment instructions.

- For ActiveMQ durable topic semantics, use `ConnectOptions` to set the
  `client-id` header on CONNECT, and `SubscriptionOptions` to set the
  durable subscription name header on SUBSCRIBE.

- If you need richer broker-specific automation (for example creating
  durable queues and bindings programmatically), consider an adapter or a
  small helper that talks to the broker's management API — that is out of
  scope for the STOMP client but can be added as an optional helper.

References
----------
- RabbitMQ documentation: <https://www.rabbitmq.com/> (see "exchanges, queues and bindings")
- ActiveMQ STOMP docs: consult the ActiveMQ documentation for the version you run for exact header names and durable subscription behavior.

