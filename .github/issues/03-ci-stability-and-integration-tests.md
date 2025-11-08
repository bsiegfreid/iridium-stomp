Title: Robust integration tests & CI stability

Body:
What: Harden CI workflow and the integration smoke test to avoid flakiness.

Why: CI must be reliable; flaky integration tests reduce trust.

Acceptance criteria:
- CI workflow uses wait/retry or management API checks for STOMP readiness.
- Smoke test reliably passes in CI across multiple runs.
- Add a short section to README describing how CI runs integration tests.

Estimate: 2-3 days
