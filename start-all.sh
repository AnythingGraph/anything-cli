#!/usr/bin/env bash
set -euo pipefail

# Start reasoning-service (Rust) and thin MCP (HTTP) together.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

export AG_WORKSPACE_ROOT="$ROOT_DIR"
export AG_REASONING_HOST="${AG_REASONING_HOST:-127.0.0.1}"
export AG_REASONING_PORT="${AG_REASONING_PORT:-8787}"
export AG_REASONING_URL="${AG_REASONING_URL:-http://${AG_REASONING_HOST}:${AG_REASONING_PORT}}"
export AG_MCP_HOST="${AG_MCP_HOST:-127.0.0.1}"
export AG_MCP_PORT="${AG_MCP_PORT:-3334}"
export AG_PAYROLL_CSV_PATH="${AG_PAYROLL_CSV_PATH:-$ROOT_DIR/data/payroll.csv}"

REASONING_PID=""
MCP_PID=""

# Kill any process listening on a TCP port (macOS/Linux via lsof).
kill_port_listeners() {
  local port="$1"
  local label="$2"
  local port_pids=""

  if ! command -v lsof >/dev/null 2>&1; then
    echo "Warning: lsof not found; cannot free port ${port} (${label})."
    return 0
  fi

  port_pids="$(lsof -ti "tcp:${port}" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -z "$port_pids" ]]; then
    return 0
  fi

  echo "Stopping existing ${label} on port ${port} (pid(s): ${port_pids}) ..."
  # shellcheck disable=SC2086
  kill ${port_pids} 2>/dev/null || true
  sleep 1

  port_pids="$(lsof -ti "tcp:${port}" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$port_pids" ]]; then
    # shellcheck disable=SC2086
    kill -9 ${port_pids} 2>/dev/null || true
    sleep 0.5
  fi
}

# Stop leftover ag-cli processes from prior runs.
kill_existing_services() {
  echo "Checking for existing ag-cli services ..."
  kill_port_listeners "$AG_REASONING_PORT" "reasoning-service"
  kill_port_listeners "$AG_MCP_PORT" "MCP HTTP"

  if command -v pkill >/dev/null 2>&1; then
    pkill -f "target/debug/reasoning-service" 2>/dev/null || true
    pkill -f "target/release/reasoning-service" 2>/dev/null || true
    pkill -f "tsx src/http.ts" 2>/dev/null || true
    pkill -f "mcp/dist/http.js" 2>/dev/null || true
  fi

  sleep 0.5
}

# Stop both services when this script exits.
cleanup() {
  echo ""
  echo "Stopping ag-cli services..."
  if [[ -n "$MCP_PID" ]] && kill -0 "$MCP_PID" 2>/dev/null; then
    kill "$MCP_PID" 2>/dev/null || true
  fi
  if [[ -n "$REASONING_PID" ]] && kill -0 "$REASONING_PID" 2>/dev/null; then
    kill "$REASONING_PID" 2>/dev/null || true
  fi
  kill_port_listeners "$AG_REASONING_PORT" "reasoning-service"
  kill_port_listeners "$AG_MCP_PORT" "MCP HTTP"
  wait 2>/dev/null || true
  echo "Done."
}

trap cleanup EXIT INT TERM

kill_existing_services

if [[ -z "${AG_SQL_DSN:-}" ]]; then
  echo "Warning: AG_SQL_DSN is not set — Postgres queries will fail."
  echo "  export AG_SQL_DSN=\"postgres://user:pass@localhost:5432/yourdb\""
fi

echo "Starting reasoning-service on ${AG_REASONING_URL} ..."
cargo run -p reasoning-service &
REASONING_PID=$!

echo "Waiting for reasoning-service ..."
ready=false
for _ in $(seq 1 90); do
  if curl -sf "${AG_REASONING_URL}/health" >/dev/null 2>&1; then
    ready=true
    break
  fi
  if ! kill -0 "$REASONING_PID" 2>/dev/null; then
    echo "reasoning-service exited unexpectedly."
    exit 1
  fi
  sleep 1
done

if [[ "$ready" != true ]]; then
  echo "reasoning-service did not become ready within 90 seconds."
  exit 1
fi

echo "reasoning-service ready:"
curl -s "${AG_REASONING_URL}/health" | python3 -m json.tool 2>/dev/null || curl -s "${AG_REASONING_URL}/health"
echo ""

if [[ ! -d "$ROOT_DIR/mcp/node_modules" ]]; then
  echo "Installing MCP dependencies..."
  (cd "$ROOT_DIR/mcp" && npm install)
fi

echo "Starting MCP HTTP on http://${AG_MCP_HOST}:${AG_MCP_PORT}/mcp ..."
(
  cd "$ROOT_DIR/mcp"
  export AG_REASONING_URL
  export AG_MCP_HOST
  export AG_MCP_PORT
  npm run dev:http
) &
MCP_PID=$!

echo ""
echo "ag-cli is running:"
echo "  reasoning-service  ${AG_REASONING_URL}"
echo "  MCP endpoint       http://${AG_MCP_HOST}:${AG_MCP_PORT}/mcp"
echo "  payroll CSV        ${AG_PAYROLL_CSV_PATH}"
echo ""
echo "Press Ctrl+C to stop both services."
echo ""

wait
