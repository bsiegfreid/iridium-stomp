Title: ACK modes and transaction support

Body:
What: Implement ACK/NACK frames and transaction frames (BEGIN/COMMIT/ABORT) in encoder/decoder and API.

Why: Required for reliable message processing and transactional semantics.

Acceptance criteria:
- Encoder/decoder support ACK, NACK, BEGIN, COMMIT, ABORT frames.
- API exposes methods to send ACK/NACK and to begin/commit/abort transactions.
- Example demonstrating CLIENT ack mode processing.

Estimate: 2-3 days
