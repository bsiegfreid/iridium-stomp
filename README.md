# iridium-stomp
[![CI](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml/badge.svg)](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml)

Asynchronous STOMP library in Rust.

## Running RabbitMQ with STOMP for local tests

A lightweight RabbitMQ instance with the STOMP plugin is provided via `docker-compose.yml` at the repository root. This is intended for local development and tests.

Quick start:

```bash
docker compose up -d
```

Check status and logs:

```bash
docker compose ps
docker compose logs -f rabbitmq
```

Access points:
- Management UI: http://localhost:15672  (user: `guest` / `guest`)
- AMQP: `localhost:5672`
- STOMP: `localhost:61613`

Stop and remove the container and data volume:

```bash
docker compose down -v
```

Notes:
- If you need different credentials, update `RABBITMQ_DEFAULT_USER` and `RABBITMQ_DEFAULT_PASS` in `docker-compose.yml`.
- The compose service enables `rabbitmq_stomp` and `rabbitmq_management` before starting the server and includes a healthcheck.

## Helper script: build, run, test, cleanup

A small helper script is provided at `scripts/test-with-rabbit.sh` that builds a RabbitMQ image with the STOMP plugin baked in, runs it locally, waits for STOMP to be ready, runs the Rust smoke test, and cleans up the container and image.

Usage (from the repository root):

```bash
./scripts/test-with-rabbit.sh
```

You can override environment variables to customize behavior:

```bash
IMAGE_NAME=iridium-rabbitmq-stomp:local \ 
	CONTAINER_NAME=iridium_rabbitmq_local \ 
	TIMEOUT_SECONDS=90 \ 
	./scripts/test-with-rabbit.sh
```

What it does:
- Builds `.github/docker/rabbitmq-stomp/Dockerfile` which enables `rabbitmq_stomp` and `rabbitmq_management` at build time.
- Runs the container mapping 61613 (STOMP), 5672 (AMQP), and 15672 (management) to localhost.
- Waits for the STOMP port to accept connections (default 60s).
- Runs the Rust integration smoke test `iridium-stomp/tests/stomp_smoke.rs`.
- Prints RabbitMQ logs if the test fails and always removes the container/image on exit.

## CI

A GitHub Actions workflow is available at `.github/workflows/ci.yml`. It builds the baked RabbitMQ image, starts it on the runner, waits for STOMP to be reachable on `localhost:61613`, and executes the same Rust smoke test.

Notes:
- The CI workflow builds and runs Docker on the runner (no external registry required).
- The CI workflow builds and runs Docker on the runner (no external registry required).
