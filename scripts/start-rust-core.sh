#!/usr/bin/env bash
# Start eduforge-core (rust) backend on :8080, then optionally the web preview.
# Requires: real Postgres + NATS (or docker-compose env). This machine has no
# psql/nats-server/docker, so it prints the exact bring-up commands instead.

set -euo pipefail
cd /workspace/apps/rust-core

# 0) bring up infra (skip if already running)
if ! curl -sf "http://127.0.0.1:4222" >/dev/null 2>&1; then
  echo "[infra] NATS not on :4222. Start one of:"
  echo "  docker run -d -p 4222:4222 nats/nats:latest -s jetstream"
  echo "  nats-server -js"
  echo ""
fi
if ! pg_isready -q 2>/dev/null; then
  echo "[infra] Postgres not ready. Start one of:"
  echo "  docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16"
  echo "  then: psql -h 127.0.0.1 -U postgres -c 'CREATE DATABASE forge;'"
  echo "        psql -h 127.0.0.1 -U postgres -d forge -c '\\i schema/001_init.sql'"
  echo "        psql -h 127.0.0.1 -U postgres -d forge -c '\\i schema/002_admin.sql'"
  echo ""
fi

# 1) load env (required vars; fail-closed otherwise)
if [ -f .env ]; then set -a; source .env; set +a; else
  echo "[env] no .env found; copy .env.example -> .env and fill real values first."
fi

# 2) build release (one-time; cargo caches)
~/.cargo/bin/cargo build --release

# 3) run core on :8080
exec ~/.cargo/bin/cargo run --release
