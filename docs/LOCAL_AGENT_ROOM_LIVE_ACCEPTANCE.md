# Live acceptance: 2026-09-29

Environment: unsigned macOS debug bundle, identifier `design.ivg.opencovibe.local`,
isolated `OPENCOVIBE_DATA_DIR=/Users/ivg/github/OpenCovibe/.local-data`.
Tests used existing subscription-backed local CLI logins; no global provider configuration,
credentials, or existing GitHub tasks were changed.

## GitHub Project lifecycle and board

- Native Retry Project setup created [Project #9](https://github.com/users/ivg-design/projects/9).
  Room ID: `5eb7e094-1aa7-488b-8ee2-b5a995ca2a25`, title Local acceptance.
- A test draft created through `gh` appeared in Todo on native Refresh board. Changing it to
  Done through `gh` moved it to Done. Title filtering hid/restored that item.
- Quit/relaunch reused Project #9 and retained the room, board and notes. Lookup by the exact
  room title returned one Project. No existing Project was modified.
- Initial Create room submission also succeeded: a second paused room, Creation acceptance,
  created [Project #10](https://github.com/users/ivg-design/projects/10) with no peers or tasks.
  Native Archive preserved that room and disabled its controls.
- Native inline task inspection showed the task's instructions and completion evidence,
  including draft items without a conventional issue URL.

## Actual Codex and Claude peers in one room

Peers were added through the native room UI, initially paused:

| Peer        | Provider/model              | Effort | Turn limit | Workspace             |
| ----------- | --------------------------- | ------ | ---------- | --------------------- |
| Codex peer  | Codex 0.153.0, gpt-5.6-luna | medium | 3          | isolated Git worktree |
| Claude peer | Claude Code, sonnet         | low    | 3          | room repository       |

The Codex worktree has branch `room/5eb7e094/d1fac199` and is registered under the isolated
profile's `worktrees/<room>/<participant>` directory. Adding either peer did not start a turn.
The two providers retained independent session/run identities across pause and relaunch.

Three explicitly bounded test drafts were added only to Project #9. Their instructions allowed
room tools only, prohibited file/command changes, and required one task per turn. Actual results:

| Draft                     | Owner       | Result                           | Canonical GitHub state            |
| ------------------------- | ----------- | -------------------------------- | --------------------------------- |
| Room runtime acceptance A | Codex peer  | `7 * 8 = 56`, `ROOM_TASK_A_OK`   | Done, Agent set, evidence in body |
| Room runtime acceptance B | Claude peer | `9 * 9 = 81`, `ROOM_TASK_B_OK`   | Done, Agent set, evidence in body |
| Room runtime acceptance C | Claude peer | `12 + 30 = 42`, `ROOM_TASK_C_OK` | Done, Agent set, evidence in body |

After A/B completed, no further human message was sent. The desktop's cooldown expired and
it offered the remaining eligible C task automatically. Claude claimed/completed C. Codex read
Claude's claim, did not duplicate the task, and spontaneously posted a directed room message
asking Claude to proceed. That message subsequently woke Claude, which acknowledged the
completed board and stopped. Both peers reached their configured 3/3 turn limits and paused.
The native board showed all four fixture drafts Done; the shared feed showed both providers'
updates and the directed message without nested threads.

Finishing recorded summary/evidence on GitHub before Done. The host offered work; peers
selected and claimed it themselves. No host-appointed manager was used.

## Failures found and repaired during acceptance

- The initial Codex connection failed with ENOENT from a broken npm wrapper. Binary resolution
  now skips candidates whose bounded version probe fails. The actual native provider probe then
  returned `OCV_CODEX_OK`; Claude's individual probe returned `OCV_CLAUDE_OK`.
- Adding a peer exposed an undefined indexed Svelte input binding. Using a defined value with
  an input handler fixed rendering; both native peer cards were then observed.
- Claude's first claim exposed a GitHub GraphQL union selection error while creating Agent.
  The write became uncertain and paused Claude. The room was paused, the query repaired, and
  the failed claim released through the native UI. Release reconciled GitHub before continuing.
- Codex initially waited for room-MCP approval. Per-process approval overrides now cover only
  the seven app-owned room tools. The mixed-provider task run then completed without those
  prompts; general provider permission behavior was not changed.

- The first agent-created task returned the same ID on both creation calls, but an old/missing
  board snapshot prevented immediate read/claim. Creation now waits for canonical item-list
  visibility, and transactional snapshot comparisons prevent an in-flight refresh erasing a
  confirmed task write. An unconfirmed creation reconciles a unique matching task directly;
  it never invokes a new create mutation during recovery.

These failed attempts were retained as recovery evidence, rather than counted as successful runs.

## Timed wakeups

A native timer targeted Claude with a 30-second interval and maximum one delivery. Its bounded
message required posting `ROOM_TIMER_OK` and stopping without other tools or file changes.
The due timer stayed at 0/1 while Claude was paused at its budget. Explicit Resume granted a new
budget; the overdue timer delivered once and Claude actually posted `ROOM_TIMER_OK` to the
shared room. The UI showed 1/1 deliveries and 1/3 turns. Later ticks did not repeat the delivery.

## Interrupted turn and restart recovery

A short no-tools probe completed as `ROOM_RESTART_PROBE`. A subsequent bounded response
was interrupted by normal desktop Quit while the native session showed running. After Quit,
SQLite retained Claude as busy with a sent pending delivery and its third reserved turn.
Relaunching the rebuilt bundle showed Claude waiting/paused with the explicit interrupted-delivery
warning, 3/3 reserved turns, and the pending message. The shared feed stayed at 25 messages;
no autonomous turn or replay occurred. Project #9, completed claims, peer identities, worktree,
and the timer's 1/1 counter persisted. Explicit Resume cleared the interrupted intent and reset
the budget to 0/3 without replaying the old response request. A fresh bounded message on the
rebuilt bundle then produced `ROOM_FINAL_BUILD_OK` through the actual room post_message tool.
The room was paused after that turn, and another relaunch retained its 28-message feed without
starting any provider work.

## Agent-created tasks and synchronization repair

On the repaired bundle, Claude reused D's original task ID, then claimed/read/finished it with
`ROOM_TASK_D_OK` and `RESULT_D=7`. In the same bounded acceptance turn it created the fresh
E draft (`PVTI_lAHOAqBX8s4BlIHIzg9iwWA`). An in-flight refresh won the first creation snapshot
comparison, so the tool returned a safe reconciliation error containing the created ID. Claude
retried with the identical title/body; two calls returned that same ID, and immediate claim/read
then succeeded. It posted `ROOM_TASK_E_OK` and finished with `RESULT_E=11` plus identical-ID
proof. It held at most one unfinished claim and created no other tasks.

Both new drafts have canonical Done status, Claude Agent ownership and completion evidence.
The native board and an independent `gh project item-list` read showed all six fixture drafts
Done. Claude stopped at 3/3 reserved turns; the room was then paused. The final shared feed has
39 messages, including failed attempts and successful recovery. The saved Project #9 and the
archived creation fixture Project #10 remain for review.

## Custom colors

Native Settings accepted Primary `#66AAEE`, Background `#161A20`, and Sidebar `#1D222B`.
Its contrast indicator changed to 15.60:1. Quit/relaunch retained those exact values.
Reset colors restored the default values and 16.46:1 contrast in the native controls.
Only the isolated acceptance profile was changed.

The final board and inline canonical evidence were also visually inspected and captured in
[the native screenshot](images/local-agent-room-board.png).

## Three-peer coding, approvals, crash recovery and hidden-window timer

The extended acceptance used [Project #11](https://github.com/users/ivg-design/projects/11),
room `b5b20af9-3af8-4621-82a5-3f8dfce71702`, and an isolated Git fixture at
`/private/tmp/opencovibe-coding-acceptance-20260929`. All three peers were observed busy
at the same time, using separate local Git worktrees and a four-turn budget each.

| Peer            | Provider/model      | Effort | Commit                                     | Files                               | Tests |
| --------------- | ------------------- | ------ | ------------------------------------------ | ----------------------------------- | ----- |
| Codex builder   | Codex, gpt-5.6-luna | medium | `e89e01e9618714bf87d87e8dbe7ec89c7839bc97` | add.py, tests/test_add.py           | 3     |
| Claude builder  | Claude, sonnet      | low    | `c40d1c9c78518cb701aa554a02ca69c5d40e7a1e` | multiply.py, tests/test_multiply.py | 4     |
| Claude reviewer | Claude, haiku       | medium | `5ef41fdbc3de9845aefa40d64d431414a3bef250` | clamp.py, tests/test_clamp.py       | 22    |

Codex created the three canonical Project tasks. Each peer claimed one task, edited only its
module and test, committed locally, sent a directed review request, and finished with evidence.
The native board and local claims showed all three Done. Review replies appeared in the shared
feed. The room was paused before using its native Merge worktree buttons to integrate all three
branches into the fixture. Independent `python3 -m unittest discover -s tests -v` on the combined
repository passed 29 tests. The merge guard rejected an untracked tests/CLAUDE.md generated
by the memory plugin; that empty context file was preserved outside the worktree before retry.
No fixture branch was pushed.

Codex needed normal approval for Git metadata outside its sandbox. The desktop showed the
permission wait and did not deliver a due timer through it. During that wait, the exact desktop
process was killed with SIGKILL. Restart retained the sent delivery intent and reserved turn,
paused the participant with an interrupted-delivery explanation, and did not replay the request.
Explicit Resume retained the provider context and granted a fresh budget. A subsequent
one-time Allow completed the authorized commit; provider permission controls stayed available.

A 30-second, one-delivery timer targeted Codex with queue-while-busy enabled. It persisted one
queued timestamp while Codex was busy, remained queued through a later permission wait,
and delivered after the turn became idle. The native window was then closed to the tray.
The host log records Window Hidden at 2026-09-29T23:51:37.083Z and the room tool's
`ROOM_TRAY_TIMER_OK` post at 23:51:48.219Z, before the app was reopened. The durable delivery
counter was 1/1 and the queued timestamp cleared. The final assistant response repeats the
marker in the feed; this is the same turn's final text, not a second timer delivery.

The audit also repaired a transcript toolbar that was showing the global effort instead of the
room participant's configured effort. Room transcripts now use a run-specific local lookup,
leave unspecified values blank, and make these controls read-only. Native sidebar navigation
confirmed Codex gpt-5.6-luna medium and Claude sonnet low, while an ordinary new chat
retained its editable model control.

## Fixed app frame and internal scrolling

The rebuilt native app was scrolled from the room summary to the bottom Task claims panel.
The sidebar, page title/actions and saved-room selector stayed fixed. Scrolling over the app
header left the accessibility tree unchanged, and the bottom content remained inside the detail
panel without exposing blank document space. The room list and details have separate scroll
containers; the create form lives inside the detail panel. The root document is viewport-bound
with overflow hidden. The result is captured in
[the fixed-frame native screenshot](images/local-agent-room-fixed-frame.png).

## Verification boundaries

The fixture Projects, completed drafts, rooms, peers, conversations and worktrees remain for review.
All acceptance rooms were paused after testing. This is an unsigned development build;
signing/notarization and distribution are outside this acceptance. The desktop must be running
for wakeups, including when hidden in the tray. Actual macOS sleep/wake and a prolonged soak
were not exercised; automated scheduler tests cover long elapsed gaps and coalesce overdue
timers into one bounded delivery. Quota/no-progress stopping is tested with deterministic
provider events rather than by deliberately consuming a real subscription quota.

Automated tests cover durable delivery reservation and restart recovery, competing claims,
atomic concurrency limits across independent SQLite connections, stale/failed boards,
paused/waiting/blocked peers, three automatic turns without work progress, timer limits and
busy queues, old-record defaults, task-creation intents, scoped MCP approvals, recipient history
filtering, atomic message acknowledgement, timer edits between planning and reservation,
delayed board reads racing confirmed writes, and clean/conflicting Git merges.

Final `npm run verify` passed: 1,540 frontend tests, lint, formatting, type checks, locale checks,
frontend build, Rust formatting and Clippy. The full Rust suite passed 870 tests with two existing
ignored tests. Svelte/locale checks retain existing warnings. The room MCP subprocess previously
passed a real initialize/tools-list/snapshot/paused-write-rejection handshake using newline JSON-RPC.

The optional `npm run doc:check` command cannot run because the upstream package script
references an absent scripts/doc-check.mjs. This does not affect `npm run verify`.
