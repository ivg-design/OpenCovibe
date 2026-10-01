# Room attachments, runtime recovery and desktop layout acceptance

September 30, 2026. Unsigned local macOS development build.

## Resulting behavior

- Room messages support image/file attachments from a native multi-file picker, file or image clipboard paste, native file drops and browser file/link drops. Files are copied to durable room storage; messages show names, sizes, thumbnails and a Finder action rather than encoded payloads. Limits are eight files, 20 MiB per file and 80 MiB per message.
- Images are supplied as image inputs to Codex and Claude. Claude PDF inputs use its document support; other files are supplied as readable local paths. Attachment delivery is reserved with its message cutoff, survives retry, and honors directed messages and side-chat membership.
- Humans can address names with spaces using `@`, select an autocomplete entry, mention multiple agents or use `@everyone`. Explicit mentions take precedence over the recipient picker. Email addresses, URLs, code and partial names do not accidentally route a message.
- The sidebar displays assigned room-agent names and uses the same sender color as the conversation. Message bubbles have stronger color borders, a matching dot and tinted surfaces. Compact header icons expose side chats without reserving a blank action row. Directed and nested messages can branch with one eligible agent and the human.
- The composer aligns its controls, grows for multiline drafts and moves the text area above intact controls in narrow panels. Tool names and completed/failed counts stay intact; long tool content wraps. Usage tables become labeled cards, heatmaps fit their panels, summary cards use available panel width, and room/setup/settings controls wrap as whole controls.
- The application frame is fixed to the window. Scrolling at enlarged webview scale stays inside panels rather than moving the toolbar and sidebar out of view.
- Room history reconciliation streams only the conversation and lifecycle fields it needs, advances past oversized tool output and retains byte offsets. It handles bounded pages and partial appended records without duplicating messages. Existing raw logs and full transcripts are preserved. Board refresh also attempts history reconciliation.
- The macOS Window menu includes native Move & Resize again after updating `muda` from 0.17.1 to 0.17.2. The minimum window is 640 by 400 so native half-screen layouts can fit smaller displays. The upstream fix is documented in [muda 0.17.2](https://github.com/tauri-apps/muda/releases/tag/muda-v0.17.2).

## Native and provider acceptance

Tests used a separate bundle identifier and isolated data directory. Copied rooms were paused with automatic continuation disabled; only disposable fixture peers were explicitly resumed for bounded acceptance turns. No attachment-test prompts were sent to the real RAV or Nemo rooms.

| Check                             | Observed result                                                                                                                                                                          |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Native multi-file picker          | PNG and text file selected together; both draft names/sizes appeared, including the image thumbnail.                                                                                     |
| Native Finder file copy and paste | Copying a PNG in Finder and pasting into the room added an image draft instead of inserting its filename.                                                                                |
| Native image clipboard            | Copying the generated image pixels in Preview and pasting into the room added a PNG image draft.                                                                                         |
| Codex provider delivery           | GPT-6 Luna identified the supplied image as green and read `ROOM_FILE_OK` from the attached text file.                                                                                   |
| Claude provider delivery          | Claude Haiku identified the same image as green and returned `CLAUDE_ATTACHMENT_OK`.                                                                                                     |
| Persistence                       | Sent attachment metadata, thumbnail and provider replies remained after restarting the isolated app.                                                                                     |
| Mentions                          | Keyboard autocomplete selected a full agent name with spaces without sending; the recipient hint matched. Directed delivery reached the selected peer.                                   |
| Side chat                         | A directed human image message created a persistent side chat containing only Claude joiner and the human.                                                                               |
| 150% interface scale              | Multiline draft grew; controls reflowed together, colors matched sidebar identities, summary labels stayed readable, tables became cards and scrolling left the application frame fixed. |
| Native tiling                     | Window menu exposed Move & Resize; Left, Right and Return to Previous Size worked.                                                                                                       |
| Board fixture                     | The three Done tasks rendered and remained accessible through internal vertical scrolling.                                                                                               |

The acceptance room was paused again after the provider turns. A pre-existing unread fixture side-chat delivery consumed the first bounded Claude resume; the next explicit resume delivered the directed attachment message and produced the expected image reply. No production room delivery was replayed.

## Actual RAV recovery

The live RAV log reproduced the reported failure at bus event 8825 (1,070,205 bytes). An isolated copy failed with the old raw catch-up reader and passed with the projected room reader. Catch-up added the missing human-readable replies, advanced the stored cursors, preserved Project #13 and pause state, and added no duplicates on a second pass.

The same reconciliation was then applied to the real room without stopping a busy peer. Its error cleared. The Lead was resumed through the app and answered the queued messages, completed its active proof-of-concept work and posted its result. The two delegates remained paused at their approval gates. The final restart was performed only after the Lead was idle with no pending delivery. In the updated main app, Project #13 refreshed successfully with eight Done items and two In Progress items; the corresponding claims correctly remained blocked on human decisions. The sidebar showed Lead, Codex delegate and Claude delegate with their matching sender colors. Native Left tiling and Return to Previous Size also passed in the main app.

## Automated verification

- `npm run verify`: 1,564 frontend tests across 55 files; lint, formatting, translations, Svelte diagnostics, frontend build, Rust formatting and Clippy passed. Svelte reported zero errors and 71 existing warnings; translations reported zero errors and 18 existing warnings.
- Full Rust library suite: 932 passed, zero failed, five opt-in tests ignored. The large-history acceptance was run separately against isolated live data.
- New coverage includes attachment persistence, MIME validation, size/count limits, attachment-only messages, cross-room identity rejection, multiple-recipient visibility, reserved-delivery cutoffs, exact image bytes, mention parsing, oversized tool records and partially appended history pages.
- The final macOS local and isolated debug bundles built successfully. The optional existing `doc:check` script still refers to an absent file; linked evidence was checked manually.

## Boundaries

This records focused desktop/provider acceptance and the actual RAV recovery, not a claim that every possible UI state was exhaustively tested. Native picker, file paste, image paste, image delivery and one generic text file were exercised end to end. Native cross-application drag gestures, real provider PDF delivery, every attachment format, sleep/wake, prolonged soak, quota exhaustion and signed distribution remain unverified. Drop handlers and link normalization were inspected and have focused helper coverage. Earlier room creation, work ownership, timers, instructions, imports and side-chat agent behavior are documented in the existing acceptance records.
