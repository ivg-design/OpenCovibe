# App UI/UX audit and remediation — October 3, 2026

This pass covers the local fork's full navigation surface. Matrix typography and green/black colors are intentional preferences. Findings are based on native macOS UI inspection plus source review; this is not a formal accessibility certification. Version 0.3.1 identifies this bundle independently of the older running Local process. All findings listed below have source fixes.

## Findings and changes

| Priority | Finding                                                                | Remediation                                                                                                                                                  |
| -------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| High     | Matrix About article used dark text on a dark background               | App theme tokens now control headings, body, links, code and prose colors.                                                                                   |
| High     | History filters and summary/sort row overflowed at 150%                | Auto-fit filter grids, wrapping date buttons and a wrapping summary toolbar.                                                                                 |
| High     | Settings tab strip clipped Remote/Debug when enlarged                  | Wrapping tabs and responsive cards; utility pages omit the redundant project tree.                                                                           |
| High     | Requests auto-opened for agent-to-agent review traffic                 | Only requests needing a human answer enter the interruption set; default is Your answer needed. Incoming peer reviews preserve the selected view.            |
| High     | Requests became unusably cramped beside chat when enlarged             | Full-height request overlay at narrow room widths; independently scrolling list/details, persistent response controls and close button.                      |
| Medium   | Request response shortcut missing from proposal entry                  | Cmd/Ctrl+Enter submits the appropriate enabled action, preserving multiline notes and server-side completion gates.                                          |
| Medium   | Tiny nested request list and pagination obscured the queue             | Full filtered list with stable selection and independent list/detail scroll areas; optional search/type/cleanup controls collapse to preserve context space. |
| Medium   | Request close control lacked a clear upper-right target                | Bordered, labeled close button in a persistent upper-right header.                                                                                           |
| Medium   | Room picker/actions occupied unnecessary rows                          | Same-height inline picker and action buttons, wrapping when space requires it.                                                                               |
| Medium   | Room settings dominated by repeated metadata and creation forms        | Collapsed objectives and Add Participant/timer forms, existing agents first, compact controls; duplicate auto-continue action removed.                       |
| Medium   | Room chat participant controls and timestamps consumed excessive space | Compact identity-colored participant chips and shorter times with full dates on hover.                                                                       |
| Medium   | Large board task controls and gaps slowed scanning                     | Reduced task spacing and compact priority/status controls.                                                                                                   |
| Medium   | Empty Teams navigation competed with Rooms                             | Hide empty legacy tab; populated legacy feature explicitly named Claude CLI teams. Project hierarchy tab labeled Projects.                                   |
| Medium   | Direct chat repeated provider/model controls in several places         | Remove duplicate hero provider selector and quick-action Model pill; retain provider control, model dropdown and slash compatibility.                        |
| Medium   | Skills/MCP/Agents fixed split panes failed at narrow widths            | Container-based stacking with bounded list scrolling and usable detail panes.                                                                                |
| Medium   | Utility page density and headings inconsistent                         | Compact page padding, Usage heading, Settings cards, History and release-note controls.                                                                      |
| Medium   | Memory editor numbers overlapped text and lines overflowed             | Nonwrapping gutter width/padding; bounded flexible content permits CodeMirror line wrapping; toolbar reflows.                                                |
| Medium   | Import browser included known observer/background noise                | Opt-in background sessions; project-based fallback names and readable previews. Discovery remains summary-only, without scanning full histories.             |
| Medium   | Legacy Teams permission/task cards exposed raw JSON                    | Structured labeled values replace JSON serialization, including nested field summaries.                                                                      |
| Medium   | Modal keyboard navigation could escape to underlying page              | Shared modal, About and import browser trap Tab, have accessible labels and restore opener focus.                                                            |
| Medium   | Settings autosave failures were debug-only                             | Visible error alert for general and CLI configuration save failures.                                                                                         |
| Low      | Explorer empty-state direction was wrong                               | Direction-neutral Choose a file to preview.                                                                                                                  |
| Low      | Agents role description implied Claude-only support                    | Copy explains Claude/Codex reusable roles and distinguishes room participants.                                                                               |
| Low      | Human sender name was fixed                                            | Persisted identity name in General settings; room sender labels update without changing stored authorization roles.                                          |

## Coverage and limits

Native inspection includes Rooms/group chat, Requests, room settings, project board, direct chat/composer, Explorer, Memory editor, Usage, History and advanced filters, Extend (Skills, MCP, Hooks, Plugins, Agents), all six Settings tabs, About and session import. Populated permission/tool-progress cards and secondary editor paths were additionally reviewed in source. Tests use an isolated Acceptance profile and paused rooms; no new provider work or GitHub board edits were triggered for layout checks.

The existing palette contrast feedback already works (Matrix foreground/background reported 16.44:1); it was retained. Large generated JS chunks and existing Svelte warnings remain engineering diagnostics, not evidence of a newly measured performance or usability defect. Import filtering operates on the current discovery result: the backend candidate cap remains 500; hiding helper sessions does not expand that cap.

## Verification

Native and automated verification results are recorded in the companion acceptance document. A running Local app must be restarted to load the new bundle; do not replace an active agent process mid-delivery.
