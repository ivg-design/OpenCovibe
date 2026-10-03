# OpenCovibe Local

This is the independently maintained **ivg-design/OpenCovibe** fork. Version **0.3.0**
starts its own release history, based on upstream 0.2.9.

It adds local Codex/Claude rooms, shared GitHub Projects, persistent requests,
timed wakeups, session continuation, side chats, attachments and a Matrix palette.
Language and appearance preferences are available in **Settings**.

Automatic update checks and upstream download prompts are disabled. Local development
builds are installed manually; a fork update channel must be validated before it is enabled.

[Fork source and issues](https://github.com/ivg-design/OpenCovibe)

Based on [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe), under Apache 2.0.
Original copyright and license notices are retained.

## Version 0.3.1

App-wide UI/UX remediation improves Matrix readability, responsive layouts, room controls,
request focus and keyboard replies, import clarity and identity settings. See the
[UI/UX audit](UI_UX_AUDIT_2026-10-03.md) and
[acceptance evidence](UI_UX_ACCEPTANCE_2026-10-03.md) for changes, validation and deployment status.

## Version 0.3.2

GitHub task-sync failures keep uncertain claims protected while leaving room messaging
and coordination available. Legacy task-sync waits recover without overriding manual
pauses or replaying interrupted deliveries. See
[BidBot recovery evidence](BIDBOT_WAIT_RECOVERY_2026-10-03.md).
