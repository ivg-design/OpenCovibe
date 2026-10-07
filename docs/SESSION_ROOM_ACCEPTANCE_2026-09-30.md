# Session rooms, conversation layout and side-chat acceptance — 2026-09-30

This record covers the local macOS development bundle and the session-room changes following
`3a2749d`. It supplements the earlier full fixture E2E and desktop acceptance records; it is
not a claim that every upstream feature or platform has been retested.

## Automated verification

- `npm run verify` passed: lint, formatting, Svelte checks, translations, 1,548 tests in
  50 files, production frontend build, Rust formatting and Clippy with warnings denied.
- Full Rust library suite: 902 passed, zero failed, two existing ignored tests.
- Svelte checks: zero errors and 73 existing warnings; translations: zero errors and
  18 existing warnings.
- The local Tauri debug application bundle built successfully.
- `git diff --check` passed. The optional `npm run doc:check` cannot run because the repository's
  existing package script points to the absent `scripts/doc-check.mjs`; documentation links
  in this record were checked manually.

Meaningful regressions cover preserved provider identity and paged history, seeded history
being context rather than new work, instruction persistence and stale-editor conflicts,
count/date timer validation and expired queued/cold-start delivery cancellation, side-chat
membership and directed-message visibility, nested branches, single-peer branches, archived
room rejection, independent unread channels, and persistence after reopening the store.
The GitHub workflow regression checks that an existing `Remediation status` field is used
consistently for reads and writes, without silently falling back to generic Status.

## Other provider and presentation checks

The settings web-search preference was toggled off/on in the native UI and persisted; provider
configuration inspection confirmed the corresponding disabled/live values. Claude's `/clear`
was exercised as a fresh session in the same working directory, retaining the previous history.
Codex model discovery through the signed bundled CLI returned the account's dynamic model list,
including GPT-6.1 Sol and GPT-6 Luna, and a bounded GPT-6 Luna inference succeeded. The launcher
no longer probes arbitrary legacy executables with `--version`. Ordinary chat messages,
streaming replies and expanded history use the same protocol-summary presentation as room
messages; actual source code and diffs retain their normal rendering.

## Native macOS acceptance

The rebuilt `OpenCovibe Local.app` was exercised through its native UI.

- Rooms opens the group conversation. Room settings and the Project board are separate routes.
  No board or participant/timer forms appear beneath the group conversation.
- Human messages appear on the right and agent messages on the left. Message content uses
  readable briefing cards or formatted Markdown. The extra room-list pane was replaced with
  a dropdown so the conversation uses the available panel width.
- The native interface was checked at 120% and 150% scale, in the large portrait and smaller
  landscape window shapes exposed by the window zoom control. The composer remains within
  the window; history scrolls internally. No horizontal scrollbars appeared in the checked
  conversation layouts. The original 120% preference was restored afterward.
- Shared room instructions were edited and saved in the test room, survived a restart, and
  remained unchanged after editing and cancelling a second draft.
- `Maximum active room agents` appears on its own settings row, with the explanation that
  local subagents are not counted.
- A disabled deadline timer was created using the native date/time field, ending October 1,
  2026 at 12:30 PM local time. Its deadline and disabled state survived restart, with zero
  deliveries. Actual deadline cancellation is covered by runtime tests; this UI check did
  not start inference from a timer.

### Mixed-provider side chats

In the existing acceptance fixture, the human branched a prior Claude message into
**Side chat acceptance**, selecting Codex joiner and Claude joiner. Creating the branch alone
started neither peer. The human posted a bounded acceptance prompt, explicitly resumed both
peers, and resumed the room with automatic task continuation disabled.

Both providers posted `SIDECHAT_OK` through room tools in that side chat and recalled the
prior **Fresh E2E 2244** room and the independently approved nine-test review. Their final
responses were also routed into that side chat. The main room retained its nine messages.

A second bounded Codex turn used `room.create_sidechat` on a message in the first side chat,
creating **Agent-created acceptance** with only Codex joiner. It posted `AGENT_BRANCH_OK`
into the nested branch. The native conversation selector opened that branch and displayed
its response, including after restarting the final bundle. The fixture was paused afterward
and its test timer remains disabled. No fixture source or GitHub tasks changed in these checks.

### Existing project continuation

The referenced **Review Nemo pivot remediation** Codex conversation was imported as the
starting point of **Nemo remediation**, retaining its provider session identity and model.
Its primary Git repository and the single linked open GitHub Project #2 were detected and
attached automatically. The full transcript remains accessible; 40 recent messages are shared
as room context from the 1,929-message source conversation.

The connected board contains 480 items. Native Refresh board completed read-only and updated
its sync timestamp; canonical remediation stages, task inspection and GitHub links appeared.
The repository, existing Project, history and two paused peers survived application restarts.
Original Codex and Claude peer each remain at zero room turns. No Nemo source files, task
statuses, project fields, worktrees or branch protections were changed during acceptance.
A previously unstarted paused peer now shows stopped rather than a false orphan-failure badge.

## Scope boundaries

- Side chats are focused conversations within a room. They preserve the existing peers'
  provider contexts and worktrees; they do not fork an independent provider session.
- Session conversion discovers the primary repository only, not every additional folder or
  repository mentioned in the transcript. Multiple linked Projects require a user choice.
- Existing external Codex automations remain separate from OpenCovibe's timers.
- The host schedules while the app is alive. A deadline prevents a new dispatch; it does not
  cancel a turn already running. Sleep/wake, prolonged soak and actual quota exhaustion remain
  unverified, as in the prior acceptance record.
- Existing Nemo Project writes were intentionally not exercised. The status-field mapping has
  a regression test; real task claims/completion were previously exercised on disposable fixture
  Projects, not on Nemo's protected remediation work.
- This is an unsigned local development build, not a notarized release.

Related evidence: [full fixture E2E](LOCAL_AGENT_ROOM_E2E_2026-09-29.md),
[live runtime acceptance](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md), and
[desktop fixes](DESKTOP_FIXES_2026-09-30.md).
