#!/usr/bin/env bash
set -euo pipefail

# Helper script to build a RabbitMQ image with STOMP baked in, run it locally,
# wait for the STOMP port to be ready, run the Rust smoke test, and cleanup.

IMAGE_NAME="${IMAGE_NAME:-iridium-rabbitmq-stomp:local}"
CONTAINER_NAME="${CONTAINER_NAME:-iridium_rabbitmq_local}"
BUILD_CONTEXT=".github/docker/rabbitmq-stomp"
RABBIT_USER="${RABBIT_USER:-guest}"
RABBIT_PASS="${RABBIT_PASS:-guest}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"

cleanup() {
  echo "Cleaning up: removing container and image..."
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  docker image rm "${IMAGE_NAME}" >/dev/null 2>&1 || true
}

trap cleanup EXIT

echo "Building RabbitMQ image with STOMP enabled: ${IMAGE_NAME}"
docker build -t "${IMAGE_NAME}" "${BUILD_CONTEXT}"

echo "Starting RabbitMQ container: ${CONTAINER_NAME}"
docker run -d --name "${CONTAINER_NAME}" \
  -e RABBITMQ_DEFAULT_USER="${RABBIT_USER}" \
  -e RABBITMQ_DEFAULT_PASS="${RABBIT_PASS}" \
  -p 5672:5672 -p 61613:61613 -p 15672:15672 \
  "${IMAGE_NAME}"

echo "Waiting up to ${TIMEOUT_SECONDS}s for STOMP port 61613 to be ready..."
start_ts=$(date +%s)
while true; do
  if (echo > /dev/tcp/localhost/61613) >/dev/null 2>&1; then
    echo "STOMP port is open"
    break
  fi
  now_ts=$(date +%s)
  elapsed=$((now_ts - start_ts))
  if [ "$elapsed" -ge "${TIMEOUT_SECONDS}" ]; then
    echo "Timed out waiting for STOMP port (waited ${elapsed}s). Dumping container logs:"
    docker logs "${CONTAINER_NAME}" || true
    exit 2
  fi
  sleep 1
done

echo "Running Rust smoke test..."
pushd iridium-stomp >/dev/null
cargo test --test stomp_smoke -- --nocapture
test_exit=$?
popd >/dev/null

if [ "$test_exit" -ne 0 ]; then
  echo "Tests failed with exit code ${test_exit}; showing RabbitMQ logs for debugging:"
  docker logs "${CONTAINER_NAME}" || true
fi

exit "$test_exit"
