# Kairos MVP — Spike results

Date: 2026-10-08 · Machine: Apple Silicon, macOS 27.0 (26A428) · Command Line Tools only (no full Xcode)

## Spike A — ScreenCaptureKit from the Rust daemon (TCC)

| Check | Result | How it was verified |
|---|---|---|
| `screencapturekit` 11 crate builds and links | ✅ | The crate's Swift bridge assumes the Xcode toolchain path for `libswiftCompatibility56.a`. `crates/kairos-platform-macos/build.rs` adds the Command Line Tools path, and `.cargo/config.toml` pins `MACOSX_DEPLOYMENT_TARGET=14.0`. |
| Frames arrive (all displays, 320 px wide, 1 fps) | ✅ | `scripts/probe-demo.sh` captured 2 displays ([1, 3]); ~1 frame/s per display once content changes, `changed_ratio` 0 on a static screen. |
| Denied permission is reported, not crashed on | ✅ | Before the grant, `SCShareableContent::get()` returned "The user declined TCCs…" and `CGPreflightScreenCaptureAccess()` returned `false`. |
| Grant applies to the **SMAppService agent** | ⏳ manual | Needs the signed bundle installed and registered (`scripts/build.sh --install`, then launch). In this run the probe ran from the terminal, so TCC attributed it to the terminal app. |
| Grant survives a rebuild | ⏳ manual | Needs an Apple Development certificate. None is installed on this machine (`security find-identity` lists 0 identities), so `build.sh` fell back to ad-hoc signing, whose designated requirement is the cdhash and changes on every build. |

**Decision:** keep the motion probe in the daemon (no fallback needed so far). If the two manual checks fail, use the documented fallback: move `ScreenMotionProbe` into Kairos.app and stream `MotionSample{changed_ratio}` to the daemon. The classifier only consumes a ratio, so its interface stays the same.

**Dirty rects:** `SCStreamFrameInfo.dirtyRects` are used as an upper bound and the 32×18 tile-luminance diff as ground truth (`min(dirty, tiles)`). Their coordinate space isn't documented for scaled output, so the ratio is normalised by the largest extent seen. Re-check this during the Key Result "Watching detection" run.

## Spike B — mic / camera "in use" flags

| Check | Result | Notes |
|---|---|---|
| `kAudioDevicePropertyDeviceIsRunningSomewhere` on every input device | ✅ reads | 2 input devices found, 2 property listeners installed, value `false` while idle. |
| `kCMIODevicePropertyDeviceIsRunningSomewhere` on every camera | ✅ reads | Returns `false` while idle. |
| No TCC prompt | ✅ | No microphone/camera prompt appeared. Only the on/off flag is read and no device is opened. |
| Zoom / Meet in a browser / FaceTime / Continuity Camera / OBS virtual camera | ⏳ manual | Run `scripts/probe-demo.sh 120` while starting each app, and check that the `mic`/`camera` columns flip within 2 s. |

**Listener vs. polling:** CoreAudio uses a property listener (on the device list and on each device's "running somewhere") that marks the state dirty, plus a 2 s poll fallback. CoreMediaIO is polled every 2 s. The worst-case detection latency is therefore 2 s + the configured delay, within the "delay + 3 s" budget.
