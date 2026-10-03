# Upstream contribution assessment — 2026-10-02

Selective contributions make sense. A single PR for this entire fork would be difficult
for upstream to review: the current diff against freshly fetched `upstream/master`
spans 151 files and more than 25,000 added lines, including local-room architecture,
tracking/governance rules, fork identity and release configuration.

Upstream [contribution guidance](https://github.com/AnyiWang/OpenCovibe/blob/master/CONTRIBUTING.md)
asks for one focused change per PR, checks with `npm run verify`, conventional commits,
both English and Chinese strings, and a branch based on `master`. These are compatible
with sharing extracted improvements from this fork.

| Candidate | Recommendation | Preparation needed |
| --- | --- | --- |
| Native macOS Move & Resize / tiling | Good first dependency PR | Upstream lockfile still uses muda 0.17.1; this fork uses 0.17.2. Extract the dependency update and include native tiling reproduction/acceptance. |
| Tool card/burst labels and responsive wrapping | Good first small PR | Extract generic layout changes; keep tool names/counts readable and verify normal/narrow/zoomed layouts against current upstream. |
| Opening large saved chats at the latest bounded page | Strong independent PR | Isolate history window/request guards and storage tail refresh; include large-log regression cases and avoid room dependencies. |
| More accurate CLI session discovery/import | Useful separate PR | Extract saved desktop titles, main-project/worktree grouping, subagent filtering and bounded previews; use generic test fixtures and preserve upstream import UX. |
| Dynamic model/effort discovery and dropdowns | Worth proposing where still missing | Separate account-derived catalogs from local-room bindings; handle provider-specific capabilities and avoid fixed account-only model lists. |
| Persistent mixed-provider rooms, requests, wakeups and GitHub Projects | Discuss design first, then staged feature PRs | Establish maintainers' interest and API/data model boundaries. Split runtime persistence, chat UI, request inbox, task tracking and wakeups; finish cross-platform, sleep/wake, soak and quota acceptance before claiming broad readiness. |
| Fork versioning, disabled upstream updates, personal defaults | Keep in this fork | These intentionally differ from the original distribution. |

Background history worker code has no current diff; check existing upstream behavior before extracting
anything we previously described as a fix.

No upstream PR or issue has been submitted. Recommended sequence: macOS tiling dependency, generic rendering fix,
large-history improvement, importer accuracy, then an architectural discussion for rooms.
A contribution can coexist with continued independent fork releases.
