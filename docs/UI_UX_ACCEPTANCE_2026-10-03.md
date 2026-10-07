# UI/UX acceptance — October 3, 2026

Version 0.3.1, feature branch `feat/local-agent-room`. Native tests use `OpenCovibe Acceptance.app` and `/private/tmp/ocv-attachments-ui-acceptance`, independent of the user's working Local profile.

## Functional native checks

- Identity name saved as Ilya Test, appeared on previous human messages, survived an app restart, then was reset to the default. Agent identity and the stored Human authorization role are unchanged.
- Cmd+Enter submitted a multiline decision response. It also accepted a synthetic verified completion proposal in a disposable paused fixture, preserved both note lines, and archived that fixture. The backend's normal completion gates ran; no live agent review was fabricated or claimed.
- With Requests closed, an incoming peer review did not reopen it. While Your answer needed was selected, another peer review did not change the view or selection. A new human decision request reopened the closed panel and selected that request.
- About body/headings/links are readable in Matrix colors. The app version is 0.3.1.

## Layout checks

The app was inspected at 100% and 150% interface scale. The second native pass caught a remaining History summary-row overflow and an overly narrow side-by-side request layout; both were repaired before final verification.

- History advanced filters, date controls, run metadata and the summary/sort toolbar reflow without horizontal scrolling at 150%.
- All six Settings tabs remain visible. Utility pages omit the project tree to give settings, history and usage the available width.
- Memory text wraps inside the editor; line-number gutters remain separate and readable.
- At narrow room widths, Requests fills the room area with an opaque background. The underlying chat is hidden until the panel closes. Scope stays visible; optional search/type/cleanup controls are collapsed. The list and request context scroll independently while multiline response controls stay visible.
- Closing Requests restores the full-width chat composer. Participant controls and the room toolbar remain compact and wrap cleanly.
- Extension search and provider/scope controls wrap at 150%; Skills/MCP/Agents split panes stack at their narrow container breakpoints.
- Room settings and board controls were checked at normal scale and reviewed for narrow-width behavior. Existing participants precede collapsed creation forms; timer creation and objective details no longer dominate the settings page.

## Automated checks and builds

- `npm run verify`: passed, including 61 frontend test files / 1,591 tests, Svelte checks, i18n validation, lint, format, production frontend build, Rust formatting and Clippy.
- Rust tests: 960 passed, 0 failed, 5 ignored. The backend was unchanged after this run.
- After the final request-filter markup refinement, lint, format, Svelte and i18n checks passed again. Existing diagnostics remain: 65 Svelte warnings and 18 i18n warnings.
- Final 0.3.1 Acceptance and Local debug app bundles both built successfully.
- `npm run doc:check` is unavailable because the repository does not contain its referenced `scripts/doc-check.mjs`; these reports were reviewed directly.

## Deployment status

The isolated native Acceptance app ran 0.3.1 for the final checks. The Local 0.3.1 bundle is ready at `src-tauri/target/debug/bundle/macos/OpenCovibe Local.app`.

The running Local process still showed 0.3.0 at the final inspection. The user was actively composing an unsent BidBot message, so Local was deliberately left open rather than losing the draft. No room or participant pause flags were changed. A normal restart after the draft is sent or saved will load 0.3.1.

## Limits

This verifies the identified audit findings, not every possible UI state or a formal accessibility certification. Populated secondary permission/tool cards were reviewed in source; no provider turns or GitHub board edits were submitted for these checks. The import backend's existing 500-candidate discovery cap is unchanged. These are development debug bundles, not notarized distribution releases.
