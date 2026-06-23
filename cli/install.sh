#!/usr/bin/env bash
# Bootstrap AnythingGraph CLI — installs @anythinggraph/cli and runs onboard for ag-cli.
set -euo pipefail

echo "AnythingGraph CLI installer (ag-cli)"
echo ""

if ! command -v node >/dev/null 2>&1; then
  echo "Error: Node.js is required. Install from https://nodejs.org/ (Node 20+ recommended)."
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "Error: npm is required."
  exit 1
fi

echo "Installing @anythinggraph/cli..."
npm install -g @anythinggraph/cli@latest

echo ""
echo "Running onboard wizard (git clone → ~/.anythinggraph/source)..."
anythinggraph onboard --install-daemon --yes

echo ""
echo "Done. Useful commands:"
echo "  anythinggraph status"
echo "  anythinggraph doctor"
echo "  anythinggraph mcp print-config"
echo ""
echo "Set credentials in ~/.anythinggraph/source/.env (copy from .env.example)."
