# Room chat, requests and sidebar acceptance — 2026-10-02

Room chat now has a persistent requests panel. Open requests, resolved history and all
requests can be filtered and searched; requests needing a human answer sort first. The
selected request retains its evidence, options, task/proposal details, reviewer and prior
answers. Its full-width multiline answer stays outside the scrolling context, with action
buttons underneath. Existing approval, review and completion guards still apply.

The room composer also uses a full-width growing textarea, with recipient, attachment and
send controls below. Enter sends; Shift+Enter adds a newline. A detached measuring textarea
avoids collapsing the live field during typing. Room polls do not reset the message feed;
reading an older message preserves that position, with an explicit jump to latest. The
initial message window is bounded, and older room messages load on demand.

Room settings contains configuration only. Requests are in chat; task claims and worktree
integration controls are on the Project board. Room settings is a button. View source
session opens the original provider conversation and its preserved full history.

The sidebar groups project → room chat → named, colored participants. Direct conversations
remain project → agent. Rooms without other standalone conversations still create a
project entry. Continuing participant sessions are associated with the primary repository.

Direct chat restores its latest bounded history page before showing the timeline. Older
history loads only after upward scrolling. A saved projection whose appended tail exceeds
the page byte limit is rebuilt rather than replaying the entire appended tail. WebKit
does not use placeholder intrinsic heights that cause visible downward catch-up scrolling.

## Native acceptance

Tests used a separate Acceptance app and profile at
`/private/tmp/ocv-attachments-ui-acceptance`. All fixture peers were paused, with automatic
continuation disabled. No real RAV or Nemo messages or requests were answered by these tests.

- A cloned room contained 1,043 messages, including stable synthetic end markers, and 120
  synthetic requests: 20 open and 100 resolved. The sidebar displayed the room and named
  colored participants under their project. Room navigation and configuration-only settings
  were observed in the packaged app.
- At 100% scale, reading earlier messages remained stable while a multiline draft grew and
  while typing continued. Open request search and selecting a request worked.
- A four-line synthetic answer was submitted through the native panel. Its approved status
  and exact response persisted, a human response appeared in the conversation, and the
  needs-answer badge changed from 20 to 19. Fixture peers remained paused.
- At 150% scale in a 1280 × 800 window, both three/four-line input areas occupied their full
  panel widths, grew without internal scrollbars, and retained visible action buttons below.
  Scrolling request evidence/options left the answer field and buttons fixed. Shift+Enter
  inserted a room-message newline without sending. Drafts were cleared afterward.
- Enlarged-scale testing caught clipped composer controls during development. The final
  build retains columns at that tested width, uses a compact send icon in narrow composers,
  and bounds the participant header so the feed retains visible space.
- Copied saved Codex (233,615,863 bytes) and Claude (53,484,330 bytes) event logs opened at
  their latest page without progressive visible scrolling. Upward scrolling in the Claude
  view loaded older history. These fixture runs were marked native/stopped to isolate saved
  history rendering from CLI import synchronization.

## Verification and limits

- `npm run verify`: 1,573 tests in 57 files passed; lint, formatting, production build,
  translations, Rust formatting and Clippy passed. Svelte checks reported zero errors and
  71 existing warnings; translations retained 18 existing warnings.
- Full Rust library suite: 950 passed, zero failed, five ignored. Coverage includes the
  exact-limit and over-limit manifest-tail behavior.
- The final responsive-only refinements passed another Svelte check, lint and packaged
  Acceptance/Local build. They did not alter backend behavior.
- Native resolved-scope dropdown interaction was not completed; scope/search ordering has
  unit coverage. Approval/review/completion branches other than the synthetic decision
  answer were not newly exercised live in this acceptance.
- Large CLI auto-sync can still take time. An initial stale imported fixture spent several
  minutes refreshing its CLI history; this change does not establish that refresh as fast.
  Saved-history acceptance above verifies rendering/positioning, not CLI-sync throughput.
- The Acceptance app was closed and its test scale restored. Real explicitly paused peers
  were not resumed. This is focused UI/history acceptance, not a new whole-project E2E claim.
- The updated Local app was restarted with no in-flight deliveries, retaining the room's
  original automatic-continuation preference. Its real RAV chat displayed the new nested
  sidebar and persistent requests panel (six needing a human answer). Lead and Codex
  delegate remained explicitly paused; no real request was answered.
