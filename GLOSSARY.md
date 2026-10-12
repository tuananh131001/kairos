# Kairos

An eye-break reminder that tracks how much rest the user owes and covers every screen when it is time to rest.

## Language

**Load meter**:
A percentage that rises while the user works and falls while they are away. At 100 % a break is due.
_Avoid_: Timer, progress

**Break**:
A period of one break length in which the full-screen overlay asks the user to rest. The load meter drains from wherever it was when the break started.
_Avoid_: Rest session, break time session

**Break length**:
The user's chosen duration for every break, fixed when each break starts.
_Avoid_: Break duration, rest time

**Scheduled break**:
A break that starts because the load meter reached 100 %.
_Avoid_: Automatic break, normal break

**Manual break**:
A break the user starts themselves from the menu, whatever the load meter shows.
_Avoid_: Early break, forced break, break now

**Postpone**:
Sending a break away for at least 5 minutes. It returns once the load meter is back at 100 %.
_Avoid_: Snooze, skip

**Pause**:
A chosen period (30 min, 2 h, 4 h or 1 day) in which the load meter is frozen and no breaks happen.
_Avoid_: Suspend, disable
