# Large-session room creation acceptance

September 30, 2026. This fixes the reported failure when creating a room from
**RAV 2.6.0 work**:

`HISTORY_PROJECTION_REQUIRED: bus event 5915 exceeds catch-up page limit: 1328780 bytes`

## Cause and change

Room seeding was reading the raw frontend catch-up feed, which is bounded to one
MiB per page. A historical tool result exceeded that limit, even though tools are
not needed to seed the shared conversation.

Seeding now uses the existing bounded history projection, pinned to one immutable
generation. It reads pages in reverse chronological order, selects user/assistant
text, counts historical messages, and retains the latest 40 in chronological order.
Messages remain bounded to 8,000 bytes plus a truncation marker. Recent peer context
and the original full-history link remain available. The event cursor comes from
the projection summary, including events that are omitted from shared messages,
so old work is not replayed after conversion. Provider identity, repository and
model are retained. History reads still complete before stopping the original
actor or changing its metadata. The frontend catch-up limit was not raised.

## Verification

- `npm run verify` passed, including 1,556 frontend tests, lint, formatting,
  Svelte, translations, production frontend build and Rust Clippy.
- Rust library suite: 921 passed, zero failed, four ignored. The new local-data
  acceptance test was executed separately and passed.
- Unit checks cover both providers, newest-message ordering across history pages,
  the 40-message cap, a watermark beyond the selected messages, preserved source
  identity/model/timestamps, empty history, bounded Unicode previews and reader
  error propagation.
- An isolated copy of the exact failing RAV run reproduced the raw catch-up error
  at event 5915. The fixed reader counted 410 historical messages, retained 40,
  and produced watermark 6061 in 6.30 seconds. Creating a room in a temporary
  SQLite store then reconciling events succeeded without replay or runtime error.
  Only copied run data and derived projection files were used in this check.
- The final local Tauri debug app bundle rebuilt successfully. Svelte has zero
  errors and 73 existing warnings; translation checks have zero errors and 18
  existing warnings. `git diff --check` passed.

## Native retry of the user's setup

Before restarting, the failed form's exact title, objective and checked Project
option were observed and restored in the rebuilt app:

- Title: **Rive Animation Viewer**.
- Objective: development and maintenance of Rive Animation Viewer project,
  fixing existing features and developing new ones.
- Source local run: `92815e23-02b8-4436-8e0f-f7389ce80bfe`.
- Source Codex thread: `01a080fc-5d5b-71a2-85ff-6913f24b98b4`.

The retry created room `115e121e-9afc-46f9-b7db-aef53900333a` with
`/Users/ivg/github/rive-animation-viewer` and `ivg-design/rive-animation-viewer`.
The room UI shows 40 historical messages, **Original conversation**, and
**Original Codex · paused**. The board UI shows the original 410-message count and
opens [GitHub Project #13](https://github.com/users/ivg-design/projects/13).
The Project creation was the checked option in the user's original failed setup.

The saved room has Project stage **ready**, no runtime error, one paused Codex
participant and zero room wakeups. The existing Nemo and acceptance rooms retain
their prior payloads apart from routine startup timestamps. No room task was sent,
no new peer was added and no agent work was resumed. This pass verifies conversion
and board connection; it does not repeat mixed-provider task execution.

Bundle: `src-tauri/target/debug/bundle/macos/OpenCovibe Local.app`.
