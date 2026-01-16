# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Implement header escaping per STOMP 1.2 spec ([#32], [#37])
  - Headers containing `\`, `\n`, `\r`, or `:` are now properly escaped/unescaped
  - Invalid escape sequences now return parse errors

### Added

- RECEIPT frame support for delivery confirmation ([#33])
  - `Frame::receipt()` builder method for requesting receipts
  - `Connection::send_frame_with_receipt()` to send with tracking
  - `Connection::wait_for_receipt()` to await confirmation with timeout
  - `Connection::send_frame_confirmed()` convenience method
  - `ConnError::ReceiptTimeout` error variant for timeout handling
- Custom CONNECT headers and version negotiation support ([#34])
  - `ConnectOptions` struct with builder methods for customizing connection
  - `Connection::connect_with_options()` for advanced connection setup
  - Support for `client-id` header (required for ActiveMQ durable subscriptions)
  - Configurable `host` header for virtual hosts
  - Configurable `accept-version` for STOMP version negotiation
  - Custom headers support for broker-specific requirements
- `Frame::get_header()` helper method for retrieving header values

## [0.1.0] - 2025-01-14

### Added

- Initial release
- Async STOMP 1.2 client with Tokio runtime
- Automatic heartbeat negotiation and management
- Transparent reconnection with exponential backoff
- Subscription management with automatic resubscription on reconnect
- ACK modes: Auto, Client (cumulative), ClientIndividual
- Transaction support (BEGIN/COMMIT/ABORT)
- Binary body handling with content-length
- Feature-gated CLI (`--features cli`)
- Comprehensive test suite (150+ tests)

[Unreleased]: https://github.com/bsiegfreid/iridium-stomp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/bsiegfreid/iridium-stomp/releases/tag/v0.1.0
[#32]: https://github.com/bsiegfreid/iridium-stomp/issues/32
[#33]: https://github.com/bsiegfreid/iridium-stomp/issues/33
[#34]: https://github.com/bsiegfreid/iridium-stomp/issues/34
[#37]: https://github.com/bsiegfreid/iridium-stomp/pull/37
