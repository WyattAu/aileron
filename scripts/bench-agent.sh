#!/usr/bin/env bash
# Era-1 S4/S5: agent-daemon metric harness.
# Measures: boot->relay-ready latency, navigate->content latency (p50/p95),
# idle RSS. Compare against playwright-mcp with: --target playwright
# (requires npx @playwright/mcp). Results: one JSON line to stdout.
#
# Usage: scripts/bench-agent.sh [--navs N] [--target aileron|playwright]
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAVS=20
TARGET="aileron"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --navs) NAVS="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    *) echo "unknown arg $1" >&2; exit 1 ;;
  esac
done

URL_A="https://example.com/"
URL_B="https://example.org/"

mcp_call() { # name args -> text
  printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"bench","version":"1"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"%s","arguments":%s}}\n' "$1" "$2" \
  | timeout 60 "$REPO/scripts/aileron-mcp" 2>/dev/null | tail -1 | python3 -c "
import json,sys
try:
    r=json.load(sys.stdin)
    print(' '.join(x.get('text','') for x in r.get('result',{}).get('content',[{}]) if isinstance(x,dict)))
except Exception: print('')"
}

if [[ "$TARGET" == "aileron" ]]; then
  # 1) boot latency: wait for 9543 from launch
  pkill -f "aileron --agent" 2>/dev/null || true; sleep 1
  t0=$(date +%s%N)
  LD_LIBRARY_PATH=/usr/lib xvfb-run -a -s "-screen 0 1280x800x24" "$REPO/target/debug/aileron" --agent >/tmp/opencode/bench-agent.stderr 2>&1 &
  BENCH_PID=$!
  for i in $(seq 1 300); do ss -tln 2>/dev/null | grep -q 9543 && break; sleep 0.05; done
  t1=$(date +%s%N)
  BOOT_MS=$(( (t1 - t0) / 1000000 ))
  sleep 3  # let the initial pane settle
else
  BOOT_MS=0
fi

# 2) navigate->content latency: same-connection navigate + execute_js(location.href)
TIMES=()
for i in $(seq 1 "$NAVS"); do
  if (( i % 2 )); then U="$URL_A"; else U="$URL_B"; fi
  PAY="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"bench\",\"version\":\"1\"}}}
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"navigate\",\"arguments\":{\"url\":\"$U\"}}}
{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"execute_js\",\"arguments\":{\"code\":\"location.href\"}}}"
  t0=$(date +%s%N)
  OUT=$(printf '%s\n' "$PAY" | timeout 90 "$REPO/scripts/aileron-mcp" 2>/dev/null | tail -1)
  t1=$(date +%s%N)
  if echo "$OUT" | grep -q "$U"; then
    TIMES+=($(( (t1 - t0) / 1000000 )))
  fi
done

# 3) idle RSS of the daemon (the aileron binary, not the xvfb-run wrapper)
RSS_KB=0
for p in $(pgrep -f "debug/aileron" 2>/dev/null); do
  R=$(ps -o rss= -p "$p" 2>/dev/null | tr -d ' ')
  [[ -n "$R" ]] && RSS_KB=$(( RSS_KB + R ))
done
RSS_MB=$(( RSS_KB / 1024 ))

python3 - "$BOOT_MS" "$RSS_MB" "${TIMES[@]:-0}" <<'EOF'
import sys, statistics
boot, rss = int(sys.argv[1]), int(sys.argv[2])
ts = [int(x) for x in sys.argv[3:]]
p50 = statistics.median(ts) if ts else 0
p95 = sorted(ts)[max(0, int(len(ts)*0.95)-1)] if ts else 0
print(f'{{"target":"{"aileron" if boot else "playwright"}","boot_ms":{boot},"nav_to_content_p50_ms":{p50},"p95_ms":{p95},"n":{len(ts)},"idle_rss_mb":{rss}}}')
EOF

if [[ "$TARGET" == "aileron" ]]; then
  kill $BENCH_PID 2>/dev/null || pkill -f "aileron --agent" || true
fi
