#!/usr/bin/env bash
# Start the eduforge web preview (next dev) on :3000.
# Reads .env.local (FORGE_API_BASE / FORGE_ADMIN_TOKEN). Prints a self-check
# for :3000 listen + a /zh smoke probe. Pair with scripts/start-rust-core.sh.

set -euo pipefail
cd /workspace/apps/web

if [ ! -d node_modules ]; then
  echo "[deps] node_modules missing; running npm install..."
  npm install
fi

if [ ! -f .env.local ]; then
  echo "[env] no .env.local; creating from .env.local.example (fill real values)."
  cp .env.local.example .env.local
fi

echo "[web] starting next dev on :3000 (admin APIs at FORGE_API_BASE=${FORGE_API_BASE:-http://127.0.0.1:8080})"
npm run dev &
WEB_PID=$!

# wait for :3000 to accept connections (max ~45s)
for i in $(seq 1 45); do
  if curl -sf -o /dev/null "http://127.0.0.1:3000/zh"; then
    break
  fi
  sleep 1
done

echo "[check] :3000 listen + /zh status:"
curl -s -o /dev/null -w "  /zh -> HTTP %{http_code}\n" "http://127.0.0.1:3000/zh" || echo "  /zh not reachable yet"
ss -ltn 2>/dev/null | grep ':3000' || echo "  :3000 not in ss (may still be compiling)"

echo "[web] running as PID $WEB_PID; Ctrl-C to stop."
wait "$WEB_PID"
