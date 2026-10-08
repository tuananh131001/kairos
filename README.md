# kairos
Free and Open-source ultra-lightweight Eye Break Reminder

Kairos is a macOS 14+ menu bar app. It shows a single **load meter** (`NN%`) that rises while you work (typing, or watching video/calls) and falls while you are away. At 100 % a full-screen break overlay appears on every display.

## Architecture

```
Kairos.app (SwiftUI, LSUIElement)          kairosd (Rust, launchd LaunchAgent)
├─ MenuBarExtra: NN%, status, pause menu   ├─ kairos-core: load meter, phases, classifier, meeting detector
├─ Settings window                         ├─ kairos-store: SQLite (WAL) in ~/Library/Application Support/Kairos
├─ Break overlay (one window per screen)   ├─ kairos-platform-macos: idle, lock, ScreenCaptureKit motion, mic/camera
├─ Permission gate, meeting notification   ├─ kairos-ipc: NDJSON protocol (v: 1)
└─ IPCClient (NWConnection) ◀── UDS ──▶    └─ kairosd: 1 s tick, socket server, checkpoints every 10 s
```

- The daemon is the single source of truth. The app only renders the pushed `State` and sends commands.
- Socket: `~/Library/Application Support/Kairos/kairosd.sock` (mode 0600; peers with another UID are rejected).
- Protocol fixtures shared by the Rust and Swift tests live in `protocol/fixtures/`.
- Plan and specs: `docs/kairos-mvp/PLAN.md`. Spike notes: `SPIKE.md`. Performance: `PERF.md`.

## Build

Requirements: Rust (stable, with the `aarch64-apple-darwin` and `x86_64-apple-darwin` targets), Xcode 15+ and XcodeGen (`brew install xcodegen`). With only the Command Line Tools installed, `build.sh` builds the app with SwiftPM instead.

```bash
scripts/build.sh            # → build/Kairos.app (universal kairosd + app, signed)
scripts/build.sh --install  # also copies it to /Applications
```

Development:

```bash
cargo test --workspace                       # core, store, IPC, daemon integration tests
scripts/probe-demo.sh 60                     # print idle / lock / motion / mic / camera every second
cd apps/macos && xcodegen generate && open Kairos.xcodeproj
cd apps/macos && swift build                 # Command Line Tools-only build of the app
swift scripts/make-icon.swift apps/macos/Kairos/Resources/AppIcon.icns  # regenerate the app icon
```

Run a throwaway daemon and app without touching your real data or login items:

```bash
target/release/kairosd --socket /tmp/k.sock --db /tmp/k.db &
KAIROS_SOCKET=/tmp/k.sock KAIROS_SKIP_LOGIN_ITEMS=1 build/Kairos.app/Contents/MacOS/Kairos
```

## Signing

Screen Recording grants are tied to the code signature. A free **Apple Development** certificate keeps the grant across rebuilds; ad-hoc signing does not.

1. In Xcode → Settings → Accounts, add your Apple ID and create an *Apple Development* certificate.
2. Find your team ID: `security find-identity -v -p codesigning` (the 10-character ID in parentheses), or check developer.apple.com → Membership.
3. Build with it:

```bash
export KAIROS_TEAM_ID=ABCDE12345                                    # used by apps/macos/project.yml
export KAIROS_SIGN_IDENTITY="Apple Development: you@example.com (ABCDE12345)"  # optional; auto-detected
scripts/build.sh --install
```

Kairos is not sandboxed and not notarized.

## Permissions

- **Screen Recording (required).** The daemon samples every display at 1 fps and 320 px wide to tell when you are watching something without touching the keyboard. Frames are processed in memory only and are never written, logged or sent anywhere. Until the permission is granted, a blocking window offers *Open System Settings* and *Quit*, and nothing is tracked.
- **Notifications (optional).** Used to offer *Pause 30 min / 2 h / 4 h* when a meeting starts. If you deny them, a 🎙 badge and the pause options appear in the menu instead.
- **Microphone/camera:** Kairos reads only the system-wide "in use" flag. It never opens a device, so macOS does not ask for this permission.

## Login items

On first launch the app registers the daemon (`SMAppService.agent`, `Contents/Library/LaunchAgents/com.kairos.daemon.plist`) and itself (`SMAppService.mainApp`). **Quit** stops both until the next login or launch. launchd restarts the daemon only after a crash (`KeepAlive.SuccessfulExit = false`).

## License

MIT
