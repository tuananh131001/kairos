# Breaks end at a fixed end time, not when the load meter reaches 0 %

A Break used to end when the load meter drained to 0 %, and its countdown was `load / 100 × break length`. With Manual breaks a Break can start at any meter value, so we now give every Break a fixed end time (start + Break length, saved with the engine state) and end it when that time passes. The meter drains from wherever it was at the rate of the Break length the Break started with, and stops at 0 %. This gives one rule for Scheduled and Manual breaks, and a countdown that never jumps (not even when the Break length is changed mid-Break). Starting a Manual break never raises the meter. A Scheduled break starts at 100 %, so for it the two rules give the same result.

## Considered Options

- **Jump the meter to 100 % on Start break.** This keeps the old "ends at 0 %" rule, but starting a Break early would make the meter worse, and a Postpone would bring the Break back almost at once.
- **Shorten the Break in proportion to the meter** (a Break started at 30 % lasts 3:00). This is consistent with the meter, but a user who chooses to rest gets less rest than they configured, and starting at 0 % gives no Break at all.

## Consequences

- Postponing a Break after the meter has already reached 0 % closes it as postponed. It is never brought back and never recorded as completed.
- After sleep, a Break whose end time has passed ends at its end time, not at the wake time.
