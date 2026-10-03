# Sidebar and composer acceptance — 2026-10-02

The sidebar now sorts projects by their displayed name, and room/direct-chat siblings and room participants alphabetically. Timestamp updates and running-state changes no longer reorder them. Duplicate names have deterministic identity tie breakers; participant colors retain their original identity index. Uncategorized remains the final bucket.

Every room and agent has a visible compact activity badge. Busy agents pulse blue; idle agents show cyan; paused agents show gray. Waiting, blocked, starting, failed and unknown states remain distinct. A room's explicit pause gate takes precedence over participant states. One shared five-second room poll refreshes the sidebar.

The room message composer keeps recipients, growing textarea, attachments and Send on one row when its container exceeds 40rem. In a narrow chat panel the textarea occupies a full row above the controls. Request-answer fields retain their separate full-width layout with buttons below.

## Automated verification

- `npm run verify` passed: lint, formatting, Svelte/TypeScript checks, localization checks, 1,583 frontend tests, production frontend build, Rust formatting and Clippy. Svelte reports 71 existing warnings and zero errors.
- Full Rust tests: 959 passed, zero failed, five ignored.
- Ordering regressions exercise changed activity timestamps, changed states, reversed refresh arrays, duplicate labels, mixed room/direct-chat siblings, participant colors and source-array immutability.
- Status regressions cover busy/idle, attention, paused-room precedence, mixed participant states, failures and requests.
- Local and isolated Acceptance macOS debug app bundles built successfully.

## Native UI verification

In the isolated Acceptance profile, project/room/participant names appear alphabetically and every room and participant displays its paused badge without overlapping the name or provider. Closing Requests gives a single-row composer; opening Requests narrows the chat and moves its textarea above the controls. Ordinary Shift+Return multiline typing grows the textarea. No provider turn was started for these layout checks.

The Local bundle was initially left on disk while both R&D agents had active deliveries. On October 3, after both became idle with no pending deliveries, R&D was briefly paused, the app quit normally, and the updated bundle launched. Resuming R&D restored both peers to idle. Native UI verified KDCustom → nemo → rive-animation-viewer and Codexabot → Codexitron alphabetical ordering, visible idle badges for the room and both peers, and the single-row composer in the wide room panel. Nemo's room and all RAV peers retain their previous pauses. Busy-state rendering is covered by automated mapping tests; a new live provider turn was not submitted for this update.
