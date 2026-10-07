# Desktop fixes and acceptance — September 30, 2026

## Changes

- **Usage chart freeze:** the synchronous Tauri usage command was scanning Codex history on the macOS UI thread. The affected commands now run on blocking workers, Codex JSONL records stream through a typed metadata parser, and concurrent chart/heatmap requests share a serialized cache. Clearing usage caches clears both providers. Token accounting is unchanged.
- **Matrix terminal theme:** Settings → General → Custom colors → Matrix terminal applies green/black surfaces and terminal typography. The five colors remain editable; the Matrix styling and edited colors persist across restart.
- **Permission defaults:** Settings → General → Default agent permissions → Bypass all permissions applies to new Claude/Codex sessions, including room agents without explicit overrides. Existing sessions use their composer permission control. Claude receives `--dangerously-skip-permissions`; Codex exec receives `--dangerously-bypass-approvals-and-sandbox`, while Codex app-server receives approval policy `never` and sandbox `danger-full-access`. The default is not changed automatically.
- **Codex continuation:** sidebar Import CLI Sessions → Codex → find a session → Import → send the next message. Search now includes the original thread ID. App-server user prompts are included while separate injected context records are omitted from displayed user bubbles. New imports use the interactive session actor, retain the original Codex thread, and preserve the recorded model. Legacy pipe continuations also retain that model when the composer supplies no override. Source discovery ignores stale responses when switching providers.

## Packaged desktop acceptance

Tested the rebuilt unsigned macOS development bundle in the normal `~/.opencovibe-local` profile, using real installed Claude and Codex CLIs.

1. The original hang sample places the main thread inside `get_global_usage_overview` and JSON decoding. The history contained approximately 50 GB across 85,061 rollout files. With the old disk cache preserved outside the profile, the repaired app opened Usage, navigated to Settings in 0.56 seconds, and applied the theme while the cold scan continued on a worker. The full cold scan took approximately four minutes; the populated chart, heatmap and model table then rendered. A repaired-process sample shows the main thread in the macOS event loop. Cache comparison found identical daily usage for all 85,058 unchanged source files, with zero mismatches; three source files changed during the comparison.
2. Matrix styling was observed in the native app. Custom colors survived a normal restart: primary `#00FF00`, background `#050805`, sidebar `#080E09`, foreground `#96D35F`, border `#4F7A28`; displayed text contrast was 11.29:1.
3. Both live CLI bypass probes completed with exit code zero and the expected `CLAUDE_BYPASS_OK` / `CODEX_BYPASS_OK` response. Prompts requested no tools or file changes. The native default selector exposes Bypass; its saved value remained Accept Edits. Existing Codex app-server policy mappings are covered by the adapter tests.
4. Imported a completed Codex fixture through the native browser, then resumed its original thread. The first legacy attempt exposed an unsupported global-model fallback; after the fix it returned `IMPORT_RESUME_OK`, correctly recalling `absolute.py`, `negate.py`, `square.py` and nine passing tests. A fresh import using the interactive actor returned `FRESH_IMPORT_OK`, recalled the room name and approved review with nine tests, and visibly completed in the desktop UI. The test session was then stopped through End Session. These bounded prompts requested no room posts, tool calls or file changes.

Import continuation evidence:

| Case   | Original Codex thread                  | OpenCovibe run                         | Transport     | Result                                |
| ------ | -------------------------------------- | -------------------------------------- | ------------- | ------------------------------------- |
| Legacy | `01a0f034-8d4b-7f73-9266-a1a07d8a6f8f` | `6739d3c7-68b7-4fa8-98f8-c34af7556196` | Pipe exec     | Correct context, exit 0               |
| Fresh  | `01a0f03a-308b-7f42-b8a0-8677fecd3d08` | `6fa9c2c1-0633-435d-8d21-f6dc42b225be` | Session actor | Correct context, exit 0; then stopped |

Local diagnostic samples, bounded probe logs, filtered continuation events and verification logs are retained under `/Users/ivg/Projects/general/opencovibe-acceptance-2026-09-30`. History caches and full conversations are not checked into this repository.

## Automated checks and boundaries

- `npm run verify` passed: 1,541 frontend tests across 48 files, lint, formatting, Svelte check, translations, frontend build, Rust formatting and Clippy. Svelte reports 0 errors and 73 existing warnings; the translation check reports 0 errors and 18 warnings.
- Rust suite: 884 passed, 0 failed, 2 existing opt-in live tests ignored.
- Local macOS debug bundle built successfully.

This is focused acceptance of the desktop fixes above. The earlier complete room workflow is documented in [the fresh room E2E record](LOCAL_AGENT_ROOM_E2E_2026-09-29.md). A first scan of a large history remains asynchronous and can take minutes. Imported conversation continuation was tested; attaching imported conversations directly as room participants was not added. Signed distribution, real sleep/wake, prolonged soak and real quota exhaustion remain outside this acceptance.
