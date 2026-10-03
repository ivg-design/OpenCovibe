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

## Follow-up: active project collapse

The initial sidebar expansion effect tracked the expanded-project set and immediately
reopened the selected room's project after manual collapse. Expansion now happens on
navigation or the first arrival of that room's folder. Room participant disclosure tracks
selection rather than the periodically refreshed room object. Native isolated acceptance
confirmed both the active RAV project and the selected room's participant list remained
collapsed across refresh intervals; reopening the project still worked. All 39 sidebar
tests passed, Svelte checks had zero errors, and packaged Acceptance/Local builds passed.

## Follow-up: request cleanup, panel controls and chat reassociation

Requests can be closed as obsolete with a recorded reason and archived/restored without
losing status, responses or evidence. Human controls and scoped active-peer MCP tools share
the same validation. Closure does not grant human approval or accept room completion.
Archived entries leave the agents' active request snapshot and do not consume the active
request limit. Inbox scopes distinguish human answers, agent work, resolved and archived
entries, with a separate request-type filter.

The room toolbar toggles Requests. Hiding keeps the component mounted so reply drafts
survive. A new actionable request opens the panel and selects that entry when hidden;
unchanged polls leave filters, selection and drafts alone. Per-room visibility, seen request
keys and divider width survive restarts. The divider supports dragging, arrow keys and reset.

Native isolated acceptance confirmed multiline draft preservation through hide/show, closure
of a disposable obsolete request with its exact reason/audit message, bulk archiving of 102
resolved records, and resizing the panel wider. Injecting one synthetic pending request into
the isolated paused fixture reopened the hidden panel and selected it. After hiding, quitting
and reopening, the panel stayed hidden; reopening restored its wider width. Fixture peers
remained paused. Archived-scope selection/restore was not newly completed through native
UI automation; filtering and restore have unit/backend coverage.

Room settings also exposes Attach existing agent to room. Candidate discovery reads metadata, not
large event logs, and compares Git common directories so linked worktrees qualify while
unrelated clones do not. Attachment retains provider identity and restores the former peer
id from retained detached settings or old message provenance. Agents start paused, history
is not replayed, and ownership validation is atomic across rooms. New removals retain peer
settings; removed timers are not automatically reinstated.

Automated verification: 1,575 frontend tests passed; the full Rust suite passed 954 tests,
zero failed, five ignored. Svelte checks reported zero errors and 71 existing warnings.
New coverage exercises legacy identity recovery, paused restoration, duplicate/foreign-room
ownership, name conflicts, archived rooms and linked-worktree repository identity.

Native reassociation acceptance used an isolated saved RAV Codex delegate. The picker
listed it as a former participant and remained open across room polls. Attaching placed its
named paused row beneath the fixture room. Removing and attaching again retained the same
participant id, model and saved provider thread; the group history stayed at 1,045 messages.
No provider turn was sent. Acceptance caught and fixed a polling reset of the picker.
The final Acceptance and Local bundles built successfully; the acceptance app was closed.
This confirms UI/store reassociation, not a newly executed provider-resume E2E.

The main Local app was restarted after checking that no room delivery was in flight.
Original automatic-continuation preferences were restored from fresh room payloads,
preserving the user's latest removals and renames. Read-only verification of the real RAV
picker listed Claude delegate, Codex delegate and Lead as former participants. No live
chat was reassociated, no real request was answered, and no participant was resumed.


## Source session onboarding and agent wording

The Huion source's saved working folder was `/Users/ivg/Projects/general`, and its
saved Codex project assignment was empty. General was therefore faithful imported
metadata, rather than a reliable KDCustom repository association. Setup now shows
that source title and folder, permits an explicit repository picker, and refreshes
linked GitHub Project choices for the selected repository. Creation preserves the
provider thread and transcript while assigning the selected working folder and
moving the agent's sidebar entry beneath the new room.

Source setup no longer opens an existing room implicitly or falls back to the last
selected room after an error. A matching source room appears as a named destination
with explicit Open or Restore original agent (paused) actions. Cancel/Back returns
to the source. Loading and failures remain in setup, without unrelated agent removal
controls. Room headings and direct-session Open room buttons name the destination.
The reassociation controls now say Attach existing agent to room, Choose existing
agent, and Attach agent to room (paused).

Native isolated acceptance copied the stopped Huion source into the paused test
profile. Create room from session showed its exact title and General folder, with
creation disabled until a repository was chosen. The native folder picker selected
KDCustom, detected its GitHub remote, and selected the existing Project #15. Creating
Session setup acceptance moved Original Codex under KDCustom → room → agent;
General's standalone source row disappeared. Its provider thread and source run id
were unchanged, its working folder became KDCustom, and room/agent stayed paused.
No GitHub project or item was created and no provider turn was sent.

After detaching this fixture agent, source setup explicitly named Session setup
acceptance, with no room settings/removal controls. Back returned to the exact source
chat. Restore original agent (paused) reopened the named room and restored the
paused participant row. The source history remained 40 shared messages.

The full frontend suite passed 1,576 tests; the full Rust suite passed 955 tests,
zero failed and five ignored. Final lint, formatting and Svelte checks passed with
zero errors and 71 existing warnings. This covers native onboarding/reassociation
and saved identity, not a new provider-resume end-to-end run.

The updated Local bundle built and was restarted with no in-flight deliveries. Only
automatic-continuation preferences were temporarily guarded, then restored into
fresh room payloads. The user's restored RAV Lead, Codex delegate and Claude delegate
remained present and paused. Native room settings showed the new agent attachment
labels. No real source was converted, detached agent restored or participant resumed.


## Room visibility, explicit names and fork identity follow-up

The main profile's `ocv:removed-cwds` still hid KDCustom. The new Huion room existed
in the store, but a removed-project tombstone hid it from the sidebar while the
room picker ignored that tombstone. The earlier onboarding acceptance missed this
case. Active room lists now share archived/hidden filtering. Creating or explicitly
restoring one room reveals its repository without reviving other removed rooms;
those remain under Hidden rooms. Their participant sessions and import aliases stay
hidden too. Sidebar removal is labeled Remove from sidebar, rather than Delete.

Creation keeps the name field full width and prevents accidental Enter submission
from setup inputs. Changing selection updates the room URL and clears stale success
notices. Explicit unavailable room URLs show an error instead of opening another room.
Generation guards discard stale list/selection responses. Room settings supports
renaming with persisted, validated names and stale-editor conflict protection.

Native final-build acceptance used a separate stopped Huion source identity in the
isolated profile. With KDCustom removed, the custom name survived native folder
selection and repository/Project discovery. Enter in the name input did not create
anything; clicking Create produced matching notice, heading, picker and sidebar
names. The original agent stayed paused and its 40-message shared history remained.
The old Session setup acceptance room stayed hidden. Renaming to Renamed room
acceptance updated the heading, picker and sidebar and survived app restart. The
final build hid the older participant/import aliases. Hidden rooms listed exactly
the old fixture; explicit Restore opened its matching URL, heading and paused
participant beneath the correct sidebar room. No provider message or GitHub write
was sent. Native dropdown switching was not completed; URL selection and hidden
restoration were observed directly.

Fork version 0.3.0 is consistent across npm, Cargo and Tauri metadata. The rail's
language/theme/palette shortcuts were removed; preferences remain in Settings.
About shows the local fork and its repository, with no update/download control.
The update banner is no longer mounted and the legacy backend update command is
disabled without a network request. Copyright and upstream credit remain.

`npm run verify` passed: 1,579 frontend tests in 58 files, lint/format/build,
translations, Rust formatting and Clippy. The full Rust suite passed 956 tests,
zero failed and five ignored. Svelte checks reported zero errors and 71 existing
warnings; translation checks reported zero errors and 18 existing warnings. Final
Acceptance and Local bundles built successfully. Native About confirmed v0.3.0,
fork identity and the absence of update actions. The isolated app was closed.

Upstream contribution candidates and preparation are recorded separately in
[UPSTREAM_CONTRIBUTIONS.md](UPSTREAM_CONTRIBUTIONS.md). No upstream PR was submitted.

The main Local app was restarted after a fresh check found no in-flight room
deliveries. Automatic-continuation preferences were temporarily guarded and restored
into fresh payloads, preserving pause/removal/name state. Native Hidden rooms →
Restore to sidebar reopened the existing Fix Huion wheel shortcuts in Rive room at
its exact id under KDCustom → room → Original Codex. Heading and picker matched;
the old KDCustom R&D room stayed hidden. The existing source and 40 shared messages
were preserved. No real provider turn was sent or agent resumed.
