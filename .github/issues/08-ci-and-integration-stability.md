Title: Improve CI and integration test stability

Body:
What: Harden CI to reduce flakiness when building the baked RabbitMQ image and running the smoke integration test.

Why: Current CI occasionally fails due to startup timing and resource limits on GitHub Actions runners.

Acceptance criteria:
- Add retries and longer health check backoff when waiting for STOMP port.
- Use GitHub Actions service containers or setup steps to reduce image build time.
- Add diagnostics logs (container logs on failure) to help triage flakes.

Estimate: 1-2 days
