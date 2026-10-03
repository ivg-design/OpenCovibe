# Fresh packaged-app end-to-end run

Result: **PASS for the exercised workflow**, 2026-09-29 EDT (2026-09-30 UTC).
The unsigned macOS development bundle at source commit
`b67cabf694f69a7c582ed1a4dd03c8570278fd84` ran actual authenticated Codex and Claude sessions.
Room actions, approvals, timers, merges and completion acceptance were performed through the
native desktop UI. SQLite, provider event history, Git and GitHub reads corroborated the results.
No application code changes were needed during this run.

## Fixture

- Room: Fresh E2E 2244, `c9d2ef77-c8a0-4b81-9905-b5e5e0fed383`.
- Newly created [GitHub Project #12](https://github.com/users/ivg-design/projects/12),
  node `PVT_kwHOAqBX8s4BlJgR`.
- Separate local Git fixture: `/private/tmp/opencovibe-e2e-20260929-2244`.
- App profile: `/Users/ivg/github/OpenCovibe/.local-data`.
- Evidence directory: `/Users/ivg/Projects/general/opencovibe-e2e-2026-09-29`.

## Observed checks

| Check | Actual result |
| --- | --- |
| Room and Project creation | Native Create room created one new paused room and Project #12; the same Project survived relaunch. |
| Independent providers | Codex `gpt-5.6-luna` / medium and Claude `haiku` / medium ran concurrently in separate Git worktrees. |
| Canonical task creation | Codex created exactly three scoped drafts through room MCP: E2E_negate, E2E_square and E2E_absolute. |
| Ownership | Codex owned negate/square; Claude owned absolute. Codex inspected live ownership and avoided Claude's active task. |
| Automatic work continuation | Only Codex received an initial human task prompt. Resumed Claude received a host task wake. After completing negate, Codex received another host task wake and completed square without another human prompt. |
| Real coding and commits | Three modules and three unittest files were authored and committed by the providers, with actual test output attached before Done. |
| Timed tray operation | A native 30-second, maximum-one-delivery timer posted E2E_TIMER_OK while the window was closed. It remained at 1/1; there is one exact tool-post marker and a separate final assistant acknowledgement from the same turn. |
| Human integration | With the room paused, native Merge worktree integrated both branches into the fixture. The combined root passed nine tests and remained clean. |
| Requesting another agent | Codex requested E2E_HELPER. Native approval created one isolated Codex low-effort helper with a one-turn budget, observed paused at 0/1 until explicit Resume. |
| Human decision | E2E_DECISION retained the Keep local response and its directed delivery. The exhausted Codex participant did not restart on that message. |
| Independent review | E2E_REVIEW woke Claude, which inspected the merged root and actually ran all nine tests before approving through respond_request. |
| Finite budgets | Codex stopped at 6/6 despite a later directed response. The approved helper made the completion proposal during its single reserved turn and stopped at 1/1. |
| Completion verification | Claude independently verified E2E_COMPLETION, including another actual nine-test run. Reviewer evidence remained distinct from the human acceptance note. |
| Acceptance guard | Accept completion remained disabled with a filled acceptance note while the room was active. Pause, fresh board refresh and native acceptance archived the room. |
| Restart and replay prevention | Full Quit terminated the desktop process. Relaunch retained all four requests, 42 messages, claims, Project, board, timers and peer identities/configuration. Ten persisted state groups compared equal; turn counters remained 6/3/1 and the timer remained 1/1. No pending delivery or replay occurred. |
| Custom palette persistence | Primary #66AAEE and Background #161A20 persisted across full Quit/relaunch. Native Reset colors restored the original default palette. |
| Fixed frame | Native detail scrolling retained the app header, Rooms page header and saved-room list; accepted history rendered inside the detail region. |

## Code evidence

| Task | Provider | Commit | Scoped paths |
| --- | --- | --- | --- |
| E2E_negate | Codex | `5aab733` | negate.py, tests/test_negate.py |
| E2E_square | Codex, automatic continuation turn | `c02273a` | square.py, tests/test_square.py |
| E2E_absolute | Claude, automatic task wake | `3d2d52b` | absolute.py, tests/test_absolute.py |

The human-controlled local merge produced root HEAD
`11e4cde54439900ddc88c37c3a2eb7ee015bfb05`. Independent root verification ran
`python3 -m unittest discover -s tests -v`: **nine tests passed**. An independent
`gh project item-list 12 --owner ivg-design --format json` read confirmed exactly three
Done items and the expected Agent ownership. No fixture branch was pushed.

Raw local evidence includes `concurrent.json`, `project12.json`, `combined-tests.log`,
`before-restart.json`, `after-restart.json`, `assertions.log` and `native-restart.log`
in the evidence directory above. Provider transcripts remain under the isolated app profile:

- Codex run `eb1d52bb-9fbe-4772-add6-4fe2e58f4632`.
- Claude run `7eb99356-6b31-4f38-9ebb-7a7fbb34cf88`.
- Helper run `c4c246d3-f429-41eb-a496-40da64896712`.

## Limits and final state

This fresh run exercised the ordinary mixed-provider lifecycle through acceptance, plus
tray timers, budget stopping and clean restart persistence. It did not repeat every prior
failure injection: interrupted-delivery recovery, uncertain GitHub writes, task release/reclaim,
rejection/correction and merge-conflict checks remain documented in the
[previous acceptance record](LOCAL_AGENT_ROOM_LIVE_ACCEPTANCE.md).

Actual macOS sleep/wake, prolonged unattended soak and deliberately exhausting a real provider
quota remain untested. The app stays an unsigned development bundle. No work is scheduled
after full Quit. The new room is archived with all three peers paused and its worktrees retained;
the app is left open for inspection. The test palette was reset to its starting defaults.
