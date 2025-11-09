WIP: start subscription API work

Branch: 6-subscription-api-message-dispatching
Date: 2025-11-09

This is a small marker file created so we can commit a WIP snapshot before implementing the Subscription API.

Planned next steps:
- Add `Connection::subscribe`/`unsubscribe` APIs
- Implement subscription bookkeeping and message dispatch
- Add unit tests and integration tests against RabbitMQ STOMP
