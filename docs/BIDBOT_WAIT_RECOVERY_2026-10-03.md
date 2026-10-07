# BidBot Claude waiting recovery — October 3, 2026

## Observed cause

In the running 0.3.1 Local app, BidBot's Clauditron had `paused=true`, state `waiting`, no pending delivery, and this persisted error:

> Task update unconfirmed: GitHub rejected request: API rate limit already exceeded for user ID 44062706.. Refresh and reconcile or release the claim before continuing.

`operations::mark_uncertain` marked the claim uncertain and paused the entire participant after a failed claim/completion/release write. The runtime correctly protects permission/interrupted-delivery waits from automatic wakeups, so later addressed messages could not revive this task-sync pause. This was a room-runtime failure policy, not an observed Claude quota or provider permission prompt.

## Live recovery

Resumed Clauditron through the native room participant control without restarting Local or interrupting Codexitron. Clauditron then responded to the existing human instructions, recorded a temporary research claim under the human's already-given authorization, and began the Upwork side-panel research. Both room peers were busy afterward. No new instruction was sent on the user's behalf and no task claim/status was manually rewritten.

## Permanent change in 0.3.2

- Failed GitHub task writes retain the uncertain claim and board error without changing participant pause state or its in-flight delivery.
- Room replies, coordination and recovery requests remain available. Automatic task continuation remains blocked by uncertain claims; another participant cannot take the claim even after a successful board refresh.
- Successful claim/completion/release recovery clears the task-sync warning once no unconfirmed claim remains. Other error types are preserved.
- Startup recovers only legacy `waiting` participants with the exact task-update error prefix and no pending delivery. Manual pauses, permission waits and interrupted deliveries retain their existing protection.

## Validation

Full Rust suite: 964 passed, 0 failed, 5 ignored. Clippy with warnings denied passed. Four regression tests cover idle/in-flight task-write failures, claim exclusivity after board refresh, fresh addressed-message delivery, explicit manual pauses, legacy recovery boundaries and warning cleanup. Native live recovery confirms Clauditron resumed and worked from the existing room messages.

The Local 0.3.2 development bundle built successfully. The running 0.3.1 app is left intact while BidBot's agents are busy; the permanent change requires a safe restart after active turns finish. No claim is falsely marked complete and no GitHub rate-limit bypass is implemented.
