# Local Agent Room implementation

Fork: ivg-design/OpenCovibe. Baseline: b070b1d00dfa3a747e6db9a245756e131c14ce96.

Product direction: retain OpenCovibe's Tauri/Svelte UI and existing Codex/Claude runtimes.
One room represents one shared objective, with independent provider sessions and a shared
GitHub Project. Reopening the room reuses its Project. No remote collaborator system or relay.

## Ownership and contracts

- Main lane: Rust room storage, GitHub client, host scheduler, IPC registration and integration.
- Palette support lane: custom palette utilities/component, root theme application, settings integration.
- Board support lane: read-only board component and room page consuming src/lib/rooms/types.ts and api.ts.
- Translation fragments from delegates are integrated by the main lane; avoid simultaneous locale edits.

Storage uses a room SQLite database under the app's data directory, without changing provider
credentials. GitHub is authoritative for task records; short-lived delivery and claim state are local.
Use OPENCOVIBE_DATA_DIR for an isolated absolute development data directory. Keep the original
app identifier/data separate during acceptance. Do not silently alter the user's CLI config.

First increment: persistent rooms, automatic recoverable per-room Project creation, read-only
board sync, shared human notes, saved palette controls, and a tested scheduler policy seam.
Provider-driven peer posting/claims, automatic turn continuation, worktree creation and full
mixed-provider acceptance require runtime wiring and must not be called complete before exercised.

## Checkpoint acceptance: 2026-09-29

The later [live acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md) covers actual
Project creation, board changes and restart reuse, and native provider connection checks.

- Frontend lint, formatting, type check, locale check, 1,540 tests and production build passed.
- Rust formatting and Clippy passed; 11 focused room tests passed. The full verification command
  reached Rust formatting before finding a new formatting difference, which was corrected and
  the Rust checks rerun successfully.
- Built an unsigned macOS debug app with identifier design.ivg.opencovibe.local.
- Native UI: created a room with Project creation unchecked, posted a visible human note,
  quit/reopened the app, and verified the same room and note. No duplicate room appeared.
- Native palette: changed Primary to #3EA8A0, verified it after restart, and verified Reset
  restored #F2B854 in the warm dark theme.
- Real authenticated GitHub repository, Project lookup and paginated board queries passed.
  GitHub Project creation was not exercised through the app; no Project was created during acceptance.
- Scheduler tests cover ready work, explicitly owned unfinished work, stale/error snapshots,
  pause/block/busy states, cooldown, and bounded timer delivery. These are policy tests;
  no provider turn or timer was dispatched.

Known next work: peer/session controls, safe runtime dispatch, persisted claims and delivery
leases, timer editing and the host loop, room tool access for both providers, worktrees, and
mixed-provider acceptance. Project creation interrupted before its network result is known
requires reconciliation; an uncertain operation deliberately does not blindly create another
Project. A recovery/attach UI is still needed for the case where no Project was actually created.

## Isolated local development

From the checkout, run `./scripts/local-room-dev.sh`. Development data stays in .local-data
and is ignored by Git. The debug bundle must also be launched with OPENCOVIBE_DATA_DIR set;
opening it directly uses the upstream default data directory. No signing or release was performed.

New UI text belongs in both messages/en.json and messages/zh-CN.json. Run repository verification
and meaningful room/storage/board policy tests. Preserve Apache notices. Development work is local;
creating a room with Project creation enabled is an explicit UI operation that writes to GitHub.
