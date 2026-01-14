#!/usr/bin/env bash
set -euo pipefail

# Helper script to build a RabbitMQ image with STOMP baked in, run it locally,
# wait for the STOMP port to be ready, run the Rust smoke test, and cleanup.

IMAGE_NAME="${IMAGE_NAME:-iridium-rabbitmq-stomp:local}"
CONTAINER_NAME="${CONTAINER_NAME:-iridium_rabbitmq_local}"
BUILD_CONTEXT=".github/docker/rabbitmq-stomp"
RABBIT_USER="${RABBIT_USER:-guest}"
RABBIT_PASS="${RABBIT_PASS:-guest}"

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

echo "Waiting for RabbitMQ management API to be ready..."
for i in $(seq 1 60); do
  if curl -sf -u "${RABBIT_USER}:${RABBIT_PASS}" http://localhost:15672/api/overview >/dev/null 2>&1; then
    echo "✓ Management API is responding (attempt $i)"
    break
  fi
  if [ "$i" -eq 60 ]; then
    echo "ERROR: Management API never became available. Dumping container logs:"
    docker logs "${CONTAINER_NAME}" || true
    exit 2
  fi
  sleep 2
done

echo "Verifying STOMP plugin is enabled..."
for i in $(seq 1 30); do
  PLUGINS=$(curl -sf -u "${RABBIT_USER}:${RABBIT_PASS}" http://localhost:15672/api/plugins 2>/dev/null || echo "")
  if echo "$PLUGINS" | grep -q '"name":"rabbitmq_stomp".*"enabled":true'; then
    echo "✓ STOMP plugin is enabled and running"
    break
  fi
  if [ "$i" -eq 30 ]; then
    echo "ERROR: STOMP plugin never became enabled. Plugin status:"
    echo "$PLUGINS"
    docker logs "${CONTAINER_NAME}" || true
    exit 2
  fi
  sleep 2
done

echo "Verifying STOMP port accepts connections..."
for i in $(seq 1 15); do
  if (echo > /dev/tcp/localhost/61613) >/dev/null 2>&1; then
    echo "✓ STOMP port 61613 is accepting connections"
    break
  fi
  if [ "$i" -eq 15 ]; then
    echo "ERROR: STOMP port never accepted connections. Dumping container logs:"
    docker logs "${CONTAINER_NAME}" || true
    exit 2
  fi
  sleep 1
done

echo "✓ RabbitMQ with STOMP is fully ready"
echo ""
echo "Running Rust smoke test..."
cargo test --test stomp_smoke -- --nocapture
test_exit=$?

if [ "$test_exit" -ne 0 ]; then
  echo "Tests failed with exit code ${test_exit}; showing RabbitMQ logs for debugging:"
  docker logs "${CONTAINER_NAME}" || true
fi

exit "$test_exit"
