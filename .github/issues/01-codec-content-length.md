---
title: "codec: support Content-length header and binary frames"
labels: ["area:codec", "good-first-issue"]
---

Problem
-------

STOMP frames may include a `content-length` header that specifies the exact number of bytes in the body. When present the codec must read exactly that many bytes and then consume the trailing NUL terminator. The current `StompCodec` implementation only treats frames as NUL-terminated, which breaks for binary bodies that contain NUL bytes and for servers that rely on `content-length` (RabbitMQ can set/use it).

Goal
----

Update the `StompCodec` so it properly handles frames with a `content-length` header (case-insensitive) while preserving the existing NUL-terminated parsing when the header is absent.

Acceptance criteria
-------------------

- Decoder recognizes a `content-length` header and will wait for exactly that many bytes for the body before consuming the trailing NUL terminator.
- Decoder continues to support legacy NUL-terminated bodies when `content-length` is absent.
- Decoder returns `Ok(None)` (need more data) when the buffer doesn't yet contain the full body according to `content-length`.
- Decoder treats non-numeric `content-length` as a parse error (clear io::Error) rather than silently mis-parsing.
- Encoder can optionally emit a `content-length` header when encoding frames with binary bodies (optional but recommended).
- Unit tests cover: body with NUL bytes using content-length; body without content-length; invalid content-length.
- Integration smoke test demonstrates round-trip messaging with RabbitMQ STOMP plugin where the broker or client uses `content-length`.

Implementation notes / tasks
--------------------------

- [ ] Parse headers into a key/value map and look for `content-length` (case-insensitive).
- [ ] If `content-length` is present and numeric, compute the required total bytes: (header_end + content-length). If the buffer lacks the specified number of bytes, return `Ok(None)`.
- [ ] After reading exactly `content-length` bytes, ensure the following byte is the NUL terminator and consume it.
- [ ] If `content-length` is absent, keep the existing NUL-terminated behavior.
- [ ] Add unit tests near `src/codec.rs` (or in `iridium-stomp/tests`) covering binary bodies and invalid header.
- [ ] (Optional) Update Encoder to include `content-length` when encoding a frame with a non-UTF8 or binary body.

Notes / edge cases
-----------------

- STOMP header names are case-insensitive; handle `content-length`, `Content-length`, etc.
- If `content-length` is present but the broker doesn't send the trailing NUL (malformed), prefer returning an error and closing the connection rather than silently proceeding.
- Be careful to avoid copying the full buffer unnecessarily; use slices and only allocate when constructing Frame owned data.

Related files
-------------

- `iridium-stomp/src/codec.rs`
- tests under `iridium-stomp/tests/` or unit tests in `iridium-stomp/src/` near `codec.rs`

If you'd like, I can implement this change and add tests — say "implement" and I'll open a patch for the codec and tests.
