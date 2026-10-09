# Kairos MVP — Performance

Budgets (NFR): daemon CPU < 2 % average and < 50 MB; app < 80 MB and ~0 % CPU with the menu closed; IPC p99 < 10 ms.

## Smoke measurements (2026-10-08, Apple Silicon, macOS 27.0, release build)

| Metric | Value | Budget |
|---|---|---|
| IPC `GetState` round trip, 500 requests | p50 0.030 ms · p99 0.067 ms · max 0.121 ms | p99 < 10 ms |
| `PostponeBreak` round trip | 0.4 ms | overlay dismissed < 200 ms (the app also hides it optimistically on click) |
| kairosd after ~60 s, 2 displays sampling | 0.1 % CPU · 7.5 MB physical footprint (21 MB RSS) | < 2 % · < 50 MB |
| Kairos.app, menu closed | 0.4 % CPU (startup included) · 20 MB physical footprint (82 MB RSS, mostly shared framework pages) | ~0 % · < 80 MB |
| Break start → overlay | BreakStarted logged 22:23:43.426; overlay captured on screen 1.5 s later (the screenshot delay was intentional) | ≤ 1 s |

## 10-minute runs (pending — need a human at the keyboard)

Run with the installed app, one mode at a time:

```bash
scripts/perf-sample.sh 600 typing
scripts/perf-sample.sh 600 video-idle
scripts/perf-sample.sh 600 static-idle
```

| Mode | Duration | kairosd CPU | kairosd peak footprint | App CPU | App peak footprint |
|---|---|---|---|---|---|
| typing | | | | | |
| idle + video playing | | | | | |
| idle + static screen | | | | | |
