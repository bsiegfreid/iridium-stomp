Title: TLS and secure connections

Body:
What: Add TLS support for STOMP connections and configuration for trust stores, client certs, and hostname verification.

Why: Production deployments require encrypted transport and configuration options to pin certificates or use system trust stores.

Acceptance criteria:
- Support for plain TCP and TLS (via native-tls or rustls behind feature flags).
- Config options for CA, client certificate, and server name verification.
- Tests or example showing secure connection to a broker.

Estimate: 1-2 days
