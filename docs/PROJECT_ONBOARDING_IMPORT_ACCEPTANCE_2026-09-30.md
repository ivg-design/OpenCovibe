# Project onboarding, session import and web preview acceptance

September 30, 2026. These checks cover the reported onboarding, import discovery,
room composer and toolbar preview issues in the local macOS build. They supplement
prior room E2E evidence rather than repeating every project feature or platform.

## Behavior

Room setup uses a native folder chooser. Nested folders resolve to their Git root;
GitHub repository names are detected from local remotes without network access.
Origin is preferred, with a dropdown when several GitHub remotes exist. Non-Git
folders produce a readable error. Repositories without a GitHub remote, or a
manual override, still allow entering owner/repository explicitly. This value is
the GitHub namespace used for the room board, not the local folder name.
Imported conversations retain their original execution directory and provider
session; existing session room setup fills in its repository automatically.

Codex discovery uses the read-only desktop catalog for saved names, activity,
models, archive status and agent source. Its append-only name index is a fallback.
Subagents and automated CLI chats are hidden by default with an explicit opt-in.
A linked worktree is associated with its main project for filtering without
mistaking every worktree conversation for a subagent. Project and search filters
run before the 500-conversation discovery cap. The UI renders 25 results at a time
with Show more. Native folder selection supports projects outside the initial list.

Large rollout previews read bounded 128 KiB head/tail windows and omit inexact
turn counts. Full selected rollouts are still imported. Names-only catalog search
falls back to the bounded transcript prompt/model. Distinct assistant updates in
the same imported turn have distinct message identifiers.

The room textarea grows with typed and pasted multiline messages, shrinks when
cleared and caps its height with internal vertical scrolling. Recipient, text
input and send control align at the bottom. Shift+Enter inserts a line; Enter
sends, with composition input handled separately.

The toolbar now says **Web preview**. It opens a running local website, such as a
project development server. A native desktop project has no such website unless
it provides one. The address starts empty, is remembered separately per project,
and clearing it forgets the saved address. Before opening a window, a bounded
local HTTP check reports a readable failure if no server responds.

## Automated verification

- `npm run verify` passed: lint, formatting, Svelte, translations, 1,556 frontend
  tests in 52 files, production frontend build, Rust formatting and Clippy.
- Rust library suite: 919 passed, zero failed, three ignored. Two ignored checks
  predate this work; the third requires this user's actual local RAV catalog.
- That live read-only RAV check was run separately and passed: 16 main chats,
  all five requested names, discovery 1.64 seconds while build processes ran.
- Svelte: zero errors and 73 existing warnings. Translation checks: zero errors
  and 18 existing warnings.
- Regressions cover remote URL forms and validation, nested folders, linked
  worktrees, missing remotes, saved-name/source filtering, search beyond the
  discovery cap, names-only fallback search, bounded large records, assistant
  identifier collisions and readable session summaries.
- The final local Tauri debug app bundle built successfully. `git diff --check`
  passed. The optional documentation script remains unavailable because its
  existing referenced script file is absent.

## Native desktop acceptance

On the packaged local macOS app, with the existing Matrix palette:

- The importer folder chooser selected `/Users/ivg/github/rive-animation-viewer`.
  It returned 16 main conversations with all five exact names from the user's
  screenshot: Add VM interaction timeline; RAV 2.6.0 work; Find zero-polling VM
  channel; Investigate global VM; Investigate RAV playback performance.
- Search from All Projects found Add VM interaction timeline by its saved name.
  Import created local run `6379c89b-a7d1-4ba2-9121-fe668b2a2ff0`, retaining
  Codex thread `01a08db4-dd8b-7a20-abfd-091550c5b90b` and the RAV repo directory.
  Both distinct assistant commentary messages rendered alongside six tool calls.
  The intermediate test-only copy was preserved at
  `/private/tmp/ocv-onboarding-import-backup-ed9d01bf` before repeating import
  with the final message-identifier fix. The source Codex transcript was untouched.
- Create room from that import populated the saved title, RAV path and
  `ivg-design/rive-animation-viewer`. It explained that agents start paused.
  Setup was cancelled; no RAV room or GitHub Project was created.
- Choosing `/Users/ivg/github/OpenCovibe/src` in room setup normalized to the
  repo root and selected `origin · ivg-design/OpenCovibe`. Selecting upstream
  switched to `AnyiWang/OpenCovibe`; cancelling a second folder chooser preserved
  that selection. The setup was cancelled without creating a room.
- A four-line pasted draft visibly expanded the composer with bottom-aligned
  controls and no horizontal scrollbar. A three-line draft typed with
  Shift+Enter also remained unsent; clearing restored the single-line input.
- `http://127.0.0.1:9` produced the readable unavailable-server error and retained
  the address form. A temporary local static server opened in the preview window
  with the heading **Web preview connected** on the final build. The test address
  was cleared and the server stopped afterward.
- The existing two room payloads matched their pre-test snapshots except for
  startup `updated_at` timestamps. All participants and rooms stayed paused;
  messages, claims, boards and wake counts were unchanged. No agent task or new
  inference was sent, and no Nemo or RAV repository files or boards were edited.

The final app remains open on the imported RAV conversation. Bundle:
`src-tauri/target/debug/bundle/macos/OpenCovibe Local.app`.

## Acceptance boundary

This verifies local discovery, one real transcript import, its room setup context,
folder controls, multiline drafts and local web preview. This pass did not resume
RAV project work, create a production board, re-import the largest RAV transcript,
or repeat real mixed-provider execution. Earlier evidence covers that execution
on disposable room fixtures. Codex discovery remains local to the configured
Codex home; conversations absent from its local catalog and rollout files cannot
be imported. Preview launch does not start a development server automatically.
