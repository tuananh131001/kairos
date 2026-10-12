# Start break: take a Manual break from the menu

## Problem Statement

Kairos only shows the break overlay once the load meter reaches 100 %. When I know a good moment to rest has arrived (a meeting just ended, I finished a task, I'm about to step away), I can't take a proper break: the overlay won't appear until the meter decides it's time. If I walk away anyway, nothing tells me how long I rested, and the next Scheduled break still arrives based on the meter. I want a way to start a Break right now, on my own terms, with the same full-screen overlay and countdown.

## Solution

A new **Start break** item (shortcut ⌘B) in the menu bar dropdown starts a **Manual break** immediately. The overlay appears on every display with the reminder text and a countdown of one full **Break length** (10:00 by default).

The load meter does not jump to 100 %. It drains from wherever it was and stops at 0 %, while the overlay stays up until the Break length has passed. **Postpone 5 min** works exactly as for a Scheduled break: the Break returns once the meter is back at 100 %, and no sooner than 5 minutes. So an accidental click costs nothing.

To make Manual and Scheduled breaks follow one rule, **every Break now ends at a fixed end time** (start + Break length) instead of when the meter reaches 0 %. A Scheduled break starts at 100 %, so it behaves exactly as before.

The menu item is disabled while a Break is showing, while Kairos is disconnected from the daemon, and while paused. When paused it reads **"Start break (in paused)"**.

## User Stories

1. As a user, I want a "Start break" item in the menu bar dropdown, so that I can take a Break whenever it suits me.
2. As a user, I want the ⌘B shortcut on that item while the menu is open, so that I can start a Break without aiming the mouse.
3. As a user, I want a Manual break to show the same full-screen overlay on every display as a Scheduled break, so that a voluntary rest gets me off the screen just as effectively.
4. As a user, I want the overlay of a Manual break to show my reminder text, so that the message is the same whichever way the Break started.
5. As a user, I want a Manual break to last one full Break length, so that I rest for the duration I configured.
6. As a user, I want the overlay countdown to start at the full Break length and tick down to 0:00, so that I know exactly when I can return.
7. As a user, I want the load meter to keep its current value when I start a Manual break, so that starting one never makes my meter worse.
8. As a user, I want the load meter to drain during a Manual break and stop at 0 %, so that the rest I take is credited.
9. As a user, I want the overlay to stay up after the meter reaches 0 % until the Break length has passed, so that I get the full rest I asked for.
10. As a user, I want to be able to start a Manual break when the meter is already at 0 %, so that I can still step away for a guided rest.
11. As a user, I want the overlay to close on its own when the Break length has passed, so that I can return to work without extra clicks.
12. As a user, I want "Postpone 5 min" on a Manual break to behave the same as on a Scheduled break, so that there is only one Postpone to understand.
13. As a user who started a Break by accident, I want Postpone to close the overlay without pulling my next Break closer, so that a misclick costs me nothing.
14. As a user who postponed a Manual break, I want the menu to say "Break postponed · mm:ss" with the time until the meter is back at 100 %, so that I know when it returns.
15. As a user, I want a postponed Break to come back once the meter is at 100 % again and at least 5 minutes have passed, so that Postpone stays predictable.
16. As a user who postpones a Break after the meter has already reached 0 %, I want that Break to be closed for good, so that it doesn't come back.
17. As a user, I want a Break I cut short by postponing after the meter reached 0 % to be saved as postponed, not completed, so that my break history reflects what I actually did.
18. As a user with a postponed Break (red 100 % in the menu bar), I want "Start break" to be enabled, so that I can take that Break now instead of waiting out the 5 minutes.
19. As a user who starts a Break while one is postponed, I want it to continue that postponed Break and keep its postpone count, so that my history shows one Break that was postponed and then taken.
20. As a user, I want "Start break" disabled while a Break is already on screen, so that I can't start a Break on top of a Break.
21. As a user, I want "Start break" disabled while Kairos is disconnected from the daemon, so that I don't click a button that can't do anything.
22. As a user who has paused Kairos, I want "Start break" greyed out and labelled "Start break (in paused)", so that I understand why it's unavailable and that Resume comes first.
23. As a user, I want pausing during a Manual break to cancel that Break and freeze the meter at its current value, exactly as it does for a Scheduled break, so that Pause works the same everywhere.
24. As a user, I want a Scheduled break to behave exactly as it does today (full 10:00 overlay, meter from 100 % to 0 %), so that this feature changes nothing I already rely on.
25. As a user who changes the Break length during a Break, I want the current Break to keep its end time and its drain rate, so that the countdown and the meter never jump while I'm resting.
26. As a user who changes the Break length during a Break, I want the new length to apply from the next Break, so that the setting still takes effect.
27. As a user whose Mac sleeps during a Break, I want the Break to have ended at its end time if that time passed while asleep, so that no stale overlay greets me when I wake the Mac.
28. As a user, I want the daemon to remember a Break's end time across a daemon restart, so that a crash or relaunch mid-Break doesn't reset or extend my countdown.
29. As a user, I want the countdown in the dropdown ("On break · m:ss left") to match the overlay countdown, so that both always agree.
30. As a user, I want a Manual break to be saved in the break history like any other Break, so that my rest counts no matter how it started.
31. As a user, I want a Manual break to show the overlay on every display even if I connect or disconnect a display during it, so that it behaves like a Scheduled break.

## Implementation Decisions

**Domain language** (see the glossary): a **Break** is now a period of one **Break length** with a fixed end time. A **Scheduled break** starts because the load meter reached 100 %. A **Manual break** starts from the menu. The two differ only in how they start. They are not stored or reported differently.

**Engine (core)**
- Breaks end at a fixed end time. The engine state gains the current Break's **end time** and the **Break length it started with**. The OnBreak drain rate comes from that stored Break length, not from the current settings. The Break ends when `now ≥ end time`, not when the meter reaches 0 %.
- During a Break the meter drains at `100 / stored Break length` per second, stopping at 0 %. It never rises during a Break, whatever the activity.
- A new public engine command, **start break (manual)**:
  - Allowed only in the Running phase, both when a Break is postponed and when none is. Otherwise it returns a new error, "not running" (when paused or already on a Break).
  - It does not change the load meter.
  - It sets the end time to `now + Break length` and reuses the existing break chain logic. A pending postponed Break is resumed with its postpone count; otherwise a new chain starts.
  - It emits the same **BreakStarted** event as a Scheduled break, with remaining seconds equal to the full Break length.
- The Scheduled break trigger is unchanged (Running, meter ≥ 100 %, past `not_before`). It now also sets the end time and the stored Break length.
- Remaining time on a Break is `end time − now`, no longer `load / 100 × Break length`.
- **Postpone** is unchanged in the general case: it sets `not_before = now + 5 min`, increments the postpone count and keeps the chain open. New rule: if the meter is at 0 % when Postpone is clicked, the engine emits **BreakEnded(postponed)** and closes the chain immediately, without setting `not_before`. The Break then never resumes and is never later recorded as completed.
- The existing rule that "a postponed chain whose meter drains to 0 % while Running ends as completed" stays as it is (rest taken counts).
- Saving settings during a Break doesn't affect the current end time or drain rate. The new values take effect from the next Break.
- After a gap in ticks (sleep), a Break whose end time has passed ends with its end time as the end timestamp, not the wake time.
- Pause during any Break cancels it (**cancelled_by_pause**), as today.

**Persistence (store)**
- A new migration adds two nullable columns to the saved engine state: the Break **end time** and the **Break length the Break started with**. Both are written and restored with the existing checkpoint, so a daemon restart mid-Break keeps the countdown.
- The break history table is unchanged. It does not record whether a Break was manual or scheduled.
- The Break history row for a Break closed by "Postpone at 0 %" ends with outcome **postponed** and is not reopened by a later Break.

**Protocol (IPC, v1)**
- New request **StartBreak**, with no payload.
- On success the reply is ok with the new state, followed by the existing **BreakStarted** event.
- On failure the reply has error code **not_running**.
- A new shared fixture for the request, used by both the Rust and the Swift round-trip tests.
- No change to existing events. The `break_remaining_s` field in the State event now comes from the end time.

**Daemon**
- Handles StartBreak like PostponeBreak: it calls the engine, records the engine events in the store, checkpoints and replies with the state. The existing behaviour of relaunching the app when a Break starts with no clients doesn't apply here, because the app sent the request.

**Mac app**
- New "Start break" menu item between "Pause for…" and "Resume", with the ⌘B shortcut.
- The menu snapshot gains a "can start break" flag. It's true when the app is connected and the phase is Running (postponed included).
- When paused, the item is disabled and its title is "Start break (in paused)".
- The app model gains a "start break" command that sends StartBreak.
- The overlay is unchanged: same view, same Postpone button. Its countdown already follows the State's remaining seconds.

**Architecture decision record (ADR)**
- An ADR records the change from "a Break ends when the meter reaches 0 %" to "a Break ends at a fixed end time". It explains why: one rule for both kinds of Break, a countdown that never jumps, and a Manual break that never raises the meter. It also lists the rejected alternatives: jumping the meter to 100 %, and a Break shortened in proportion to the meter.
- The MVP plan's Break specs and diagrams that state "the countdown is `load / 100 × break_seconds`" and "OnBreak → Running when load reaches 0" are updated to match.

## Testing Decisions

- Tests check external behaviour only: what the user would see (phase, meter value, remaining seconds, menu state) and what is persisted (break history rows, restored state). They never assert on private fields or the order of internal calls.
- **Engine specs** (primary seam) use the existing fake clock and the `started_engine` / `run` helpers. New specs, in the style of the existing Meter, Break and Pause specs:
  - A Manual break at 30 % drains to 0 % at 3:00 and stays on break until 10:00, then ends with the outcome completed.
  - A Manual break at 0 % lasts 10:00.
  - A Scheduled break still lasts 10:00 and ends at 0 %, a regression check.
  - Start break is refused while paused and while on a Break.
  - Start break while postponed resumes the chain and keeps the postpone count.
  - Postpone early in a Manual break: the Break returns only when the meter is back at 100 % and 5 min have passed.
  - Postpone at 0 % emits BreakEnded(postponed), clears the chain and doesn't later emit completed.
  - Changing the Break length mid-Break keeps the end time and the drain rate, and the next Break uses the new length.
  - A tick after a long gap past the end time ends the Break at the end time.
  - Pause during a Manual break cancels it and freezes the meter.
- **Daemon integration test** (second seam), reusing the existing full-cycle harness with a temporary database, socket and fake probes:
  - The StartBreak request leads to a BreakStarted event and a State in the OnBreak phase.
  - The request is refused with `not_running` while paused or on a Break.
  - The break history row ends as completed, or as postponed for Postpone at 0 %.
  - Restarting the daemon mid-Break restores the same remaining time. The prior art is the existing test that restores a pause after a restart.
- **Protocol fixtures:** the new request fixture is covered automatically by the existing Rust and Swift round-trip tests. The minimum fixture count is raised by one.
- **App tests** (third seam), in the style of the existing menu snapshot and Formatting tests:
  - "Can start break" is true for Running and postponed, and false for OnBreak, Paused and disconnected.
  - The title reads "Start break (in paused)" when paused.
- The case walkthrough in this folder (the break cases page) is the reference for expected timelines.

## Out of Scope

- Recording whether a Break was manual or scheduled in the break history, or any stats or history view.
- A global hotkey that works outside the menu.
- A "Cancel break" or "Skip" button on the overlay. Postpone remains the only overlay action.
- Starting a Break while paused, including "end the pause and start a Break" in one click.
- A choice of Manual break length (for example 5 min or 20 min). A Manual break always lasts the configured Break length.
- Changing Postpone's 5-minute duration or limiting consecutive Postpones.
- Starting a Break from the meeting notification.

## Further Notes

- Glossary terms used in this spec: Load meter, Break, Break length, Scheduled break, Manual break, Postpone and Pause.
- Visual walkthrough of every case (A–I): the break cases page in this folder.
- Behaviour change for existing users: none in practice. A Scheduled break starts at 100 %, so ending it at a fixed time and ending it when the meter reaches 0 % coincide. The only visible differences are that changing the Break length mid-Break no longer makes the countdown jump, and that postponing at 0 % is recorded as postponed instead of completed.
