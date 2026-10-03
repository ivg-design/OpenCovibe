# Room agent settings acceptance — September 30, 2026

This check covers agent editing, model and effort dropdowns, renaming, and optional total
turn limits in the local macOS development build. It supplements the earlier room E2E
records; it does not retest every upstream feature or platform.

## Behavior

Room settings → **Edit agent** exposes the name, model, reasoning effort and optional
**Turns before pause** limit. A name-only edit preserves the participant's current activity.
Changing model, effort or the limit requires the participant to be paused with no in-flight
reservation. Saving never resumes an agent or resets its turn count. Names must be nonempty
and distinct within the room; conflicting edits are rejected rather than overwriting newer
settings. Provider identity, session history, worktree, project and claims remain connected.

Both add and edit forms use the existing provider model catalogs. Effort choices come from
the selected model's advertised capabilities. Refresh models refreshes those catalogs;
unknown imported settings remain visible, and a model switch clears incompatible effort.
Models without effort support explain that effort is automatic or unavailable.

Newly added and newly imported agents have no total room turn limit by default. Previously
saved finite limits are retained. The optional limit counts starts from room messages,
automatic task continuation and timers together. It pauses the individual participant at
its limit; explicit Resume resets its count. A timer's own delivery limit or end date remains
independent. Disabling a reached limit clears its obsolete budget warning without clearing
other provider errors or silently resuming work.

## Verification

- `npm run verify` passed: lint, formatting, Svelte and translations, 1,552 frontend tests
  in 51 files, production frontend build, Rust formatting and Clippy with warnings denied.
- Full Rust library suite: 909 passed, zero failed, two existing ignored tests.
- Svelte and translation checks have zero errors, with 73 and 18 existing warnings.
- The local Tauri debug app bundle built successfully; `git diff --check` passed.
- Regressions cover unlimited dispatch beyond the old limit, independent timer exhaustion,
  saved configuration after reopening storage, unchanged identity/claims/history/count,
  rejected active configuration changes, pending reservations, archived rooms, stale edits,
  stale budget warnings, and renaming a busy agent while retaining its delivery and ownership.
- Model capability checks use newly discovered model and effort names, provider defaults,
  unavailable catalogs, and incompatible effort switches.

## Native desktop checks

In the disposable **Session room acceptance** fixture:

- The existing Codex agent was changed to **GPT-6.1 Sol**, medium effort, with its total
  limit disabled. The saved room and run metadata agreed, retaining its original provider
  thread, project, claims, history and count of one. A draft change to GPT-5.5 was cancelled.
- Claude model and supported effort dropdowns worked; an Opus model and max effort were saved
  and survived restart. Selecting Haiku explains that effort is automatic.
- A subsequent change to **GPT-5.6 Luna**, low effort, was followed by a real resumed provider
  turn. The agent replied `MODEL_EDIT_OK` and recalled Fresh E2E 2244 and the earlier approved
  nine-test review. Its original Codex thread was retained. Only this requested fixture reply
  was delivered; no repository, task or GitHub write was requested.
- Renaming to **Codex continuation checker** succeeded while the participant's own pause flag
  was false and the room remained paused. The name appeared in the room and survived restart;
  session identity and turn count were unchanged. A duplicate **Claude joiner** name displayed
  a readable rejection and kept the editor open. Busy-turn renaming is covered by regression
  tests rather than an additional live inference test.
- The editor was viewed in the large portrait and smaller landscape window layouts at the
  existing 120% scale. Controls reflowed within the panel and scrolling stayed inside it.
- Fixture display names and lightweight models were restored afterward: Codex joiner uses
  GPT-6 Luna/low, Claude joiner uses Haiku/provider-default effort, each retaining its explicit
  one-turn fixture limit. The fixture and real Nemo room remain paused. Nemo's agents retain
  their prior settings and zero room turns; its project, claims and seeded history are unchanged.

The built application is an unsigned local development bundle. Existing broader limits in
`SESSION_ROOM_ACCEPTANCE_2026-09-30.md` still apply. The existing optional doc-check script
references an absent file; evidence links were checked manually.
