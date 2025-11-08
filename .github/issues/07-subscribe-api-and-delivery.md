Title: Subscribe API and message delivery guarantees

Body:
What: Design and implement a high-level subscribe API that returns a Stream of messages, supports durable subscriptions (where applicable), and exposes ack modes.

Why: Applications need an ergonomic way to receive messages and process them with proper acknowledgements.

Acceptance criteria:
- API to subscribe to a destination and receive a Stream<Result<Message, Error>>.
- Support for CLIENT and CLIENT_INDIVIDUAL ack modes.
- Examples and tests demonstrating message flow and ack behavior.

Estimate: 2-4 days
