# Live acceptance: 2026-09-29

Environment: unsigned macOS debug bundle, identifier design.ivg.opencovibe.local,
isolated OPENCOVIBE_DATA_DIR=/Users/ivg/github/OpenCovibe/.local-data.

## GitHub Project lifecycle and board

- From the native room UI, Retry Project setup created Project #9:
  https://github.com/users/ivg-design/projects/9.
- Room ID: 5eb7e094-1aa7-488b-8ee2-b5a995ca2a25.
- Created a test draft through gh in this Project, title “Acceptance fixture: board refresh”.
  Set Status to Todo. Native Refresh board displayed that title in Todo.
- Quit and relaunched the app. Selected room reused Project #9 and retained its board and notes.
  GitHub lookup by the exact room title returned one Project, number 9.
- Changed the draft Status to Done through gh. Native refresh moved it to Done.
- Title filter hid the unmatched item; clearing it restored the item.
- Project and completed draft remain as acceptance fixtures. No existing Project was modified.

This exercised Project setup for a saved room. Automatic Project setup in the initial
Create room submission and interrupted-write recovery still require separate acceptance.

## Provider connections

- Claude: sent a bounded, no-tools connection prompt through the native Chat screen.
  Actual response: OCV_CLAUDE_OK. App showed completed turn and no tool calls.
- Codex: initial native probe failed with ENOENT from the npm wrapper in ~/.local/bin.
  Fixed binary resolution to skip candidates whose bounded version probe fails, consistently
  across auth/version diagnostics, model discovery, plugin lookup and session dispatch.
  No global installation or credentials were changed.
- Rebuilt and relaunched the native app. Codex detected version 0.153.0 and its existing
  ChatGPT login; live model discovery populated the picker. Used GPT-5.6-Luna for the probe.
  Actual app-server response: OCV_CODEX_OK. App showed a completed turn and no tool calls.
- Two focused regression tests passed: broken-first/healthy-next selection and bounded
  hung-probe termination. Rust formatting and Clippy passed; the native debug bundle built.

These are individual provider tests. They do not establish shared-room agent participation,
task claims, automatic continuation, or timed wakeups; that runtime wiring is pending.
