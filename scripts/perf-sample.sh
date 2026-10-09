#!/usr/bin/env bash
set -euo pipefail

DURATION="${1:-600}"
LABEL="${2:-run}"
PID="$(pgrep -x kairosd | head -n 1 || true)"
APP_PID="$(pgrep -x Kairos | head -n 1 || true)"
if [ -z "$PID" ]; then
  echo "kairosd is not running" >&2
  exit 1
fi

start_cpu() { ps -o time= -p "$1" | awk -F'[:.]' '{ if (NF==4) print ($1*3600)+($2*60)+$3+($4/100); else print ($1*60)+$2+($3/100) }'; }
footprint_mb() { footprint "$1" 2>/dev/null | awk '/Footprint:/ {v=$(NF-4); u=$(NF-3); if (u=="KB") v/=1024; if (u=="GB") v*=1024; print v; exit}'; }

d0="$(start_cpu "$PID")"
a0="$( [ -n "$APP_PID" ] && start_cpu "$APP_PID" || echo 0)"
peak_d=0
peak_a=0
for ((i = 0; i < DURATION; i += 10)); do
  sleep 10
  f="$(footprint_mb "$PID")"; peak_d="$(echo "$f $peak_d" | awk '{print ($1>$2)?$1:$2}')"
  if [ -n "$APP_PID" ]; then
    f="$(footprint_mb "$APP_PID")"; peak_a="$(echo "$f $peak_a" | awk '{print ($1>$2)?$1:$2}')"
  fi
done
d1="$(start_cpu "$PID")"
a1="$( [ -n "$APP_PID" ] && start_cpu "$APP_PID" || echo 0)"
awk -v l="$LABEL" -v t="$DURATION" -v d0="$d0" -v d1="$d1" -v a0="$a0" -v a1="$a1" -v pd="$peak_d" -v pa="$peak_a" 'BEGIN {
  printf "| %s | %d s | %.2f %% | %.1f MB | %.2f %% | %.1f MB |\n", l, t, (d1-d0)/t*100, pd, (a1-a0)/t*100, pa
}'
