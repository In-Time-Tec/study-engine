#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Ensure Rust toolchain is on PATH
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

# Build the backend first so startup errors surface immediately
echo "Building study-engine-cli..."
cargo build --manifest-path "$SCRIPT_DIR/study-engine-cli/Cargo.toml" 2>&1

cleanup() {
  kill "$BACKEND_PID" "$FRONTEND_PID" 2>/dev/null
  wait "$BACKEND_PID" "$FRONTEND_PID" 2>/dev/null
  echo "Stopped."
}
trap cleanup EXIT INT TERM

echo "Starting backend on :3001..."
"$SCRIPT_DIR/study-engine-cli/target/debug/study-engine" serve --port 3001 &
BACKEND_PID=$!

echo "Starting UI on :5173..."
cd "$SCRIPT_DIR/study-engine-ui" && npm run dev &
FRONTEND_PID=$!

echo ""
echo "  UI  → http://localhost:5173"
echo "  API → http://localhost:3001"
echo ""
echo "Ctrl-C to stop both."

wait
