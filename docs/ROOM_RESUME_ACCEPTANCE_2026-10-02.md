# Paused room and imported Codex recovery — 2026-10-02

## Report and correction

R&D had both participants unpaused while the room itself remained paused. Its human
messages were saved, but room-level pause correctly prevented delivery. The composer
now displays the room pause state with a Resume room button and labels submission
Queue message. Resuming only a participant cannot hide the room pause notice.

After room resume, the fresh Codex participant replied. The imported participant
failed startup because its original conversation was still held by another Codex
app-server writer. Startup JSON-RPC errors were ignored, leaving a queued delivery
waiting until the 45-second timeout. The protocol now reports initialization/open
errors immediately; the actor rejects queued and subsequent sends, records failure,
and the room chat shows the named participant's error.

Codex resume now requests metadata only with `excludeTurns`, while retaining the full
provider history and passing the room's selected repository. The UI already has its
own paged history. Supported experimental capabilities are declared at initialization.
Fork responses likewise exclude history hydration and defer automatic goal continuation
until an explicit turn.

## Recovery performed

With explicit user approval, the imported participant continued from a provider-supported
full-history fork. The original Codex conversation and original OpenCovibe run were
preserved. Read-only pagination verified that the original and fork expose the same
53 turn IDs. The copied OpenCovibe run contains 3,971 conversation/tool events; lifecycle
events were excluded and run IDs and sequence numbers were rewritten, matching the
existing fork contract. The participant kept its identity, name, model, effort,
repository, room messages and task relationships. Its history cursor starts after the
copied events, so the room does not duplicate imported output.

The room was paused and the app quit between completed turns for the local update.
Recovery metadata and the pre-change room record are saved locally under
`~/.opencovibe-local/recovery/2026-10-02-rd-codexitron`. The rejected messages were made
unread for the copy; no new human message was submitted. The updated Local app was
opened and the copied participant and room resumed through the native UI.

## Verification

- Full frontend suite: 1,579 passed. Lint, formatting, Svelte check, i18n, production
  frontend build and Rust Clippy passed. Svelte retains 71 existing warnings.
- Full Rust suite: 959 passed, zero failed, five ignored. Regression coverage includes
  startup rejection, metadata-only resume with selected cwd, fork options, and retention
  of messages when an agent resumes inside a paused room.
- Native isolated room: a multiline message submitted with Queue message stayed in the
  feed; Resume room removed the pause banner and restored Send message. Explicitly
  paused fixture participants stayed paused. The fixture room was paused again afterward.
- Actual Local room: Codexitron's copied session reached running/busy, with no startup
  error, and posted a room reply acknowledging review of Codexabot's reconnect change.
  The reply appeared in the native group chat at 22:48:14 EDT. R&D and both participants
  were left unpaused. Codexabot had already replied and completed its bounded investigation.

This verifies room delivery and imported-session recovery. It does not certify the
Bluetooth change's hardware behavior or install that product build.
