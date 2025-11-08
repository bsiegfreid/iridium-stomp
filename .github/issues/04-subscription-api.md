Title: Subscription API & message dispatching

Body:
What: Implement high-level subscription API: subscribe -> Stream / Receiver, unsubscribe, and ack support.

Why: Users need ergonomic subscribe/publish semantics.

Acceptance criteria:
- API to subscribe returns an async Stream or `mpsc::Receiver<Frame>`.
- Support for ack modes (auto, client, client-individual) with ack/nack methods.
- Integration test demonstrating subscribe -> receive -> ack.

Estimate: 2-4 days
