# Room wake and project tracking acceptance — 2026-10-01

Fresh human broadcasts, Everyone picker messages, and named mentions now make automatically
blocked or no-progress agents eligible for a message turn. Explicit pauses, room pauses,
provider permission waits, quota failures, unresolved deliveries, and optional turn limits
remain guarded. The scheduler also reconsiders unread human input after an in-flight turn
finishes. This does not clear blocked task claims or decide pending human requests.

Every wake includes a built-in tracking instruction: create detailed repository Issues,
claim before work, record meaningful milestones and evidence, attach available commit/PR
references, update ownership/status through room tools, and enrich/convert legacy drafts
when taking them on. New descriptions require Outcome, Work, Acceptance criteria, Progress,
and References. Project-specific additions remain editable in Room settings.

The board provides a compact list and a status-column view, completion strip, status counts,
search across issue text/numbers/labels/PR references, owner and priority pickers, bounded
pagination, live claim blockers and renamed owner identities, and task details with progress,
evidence and Issue update history. Columns reflow instead of scrolling horizontally; list
rows reflow in narrow panels. Collapsed headers preserve space for tasks.

## Observed acceptance

- Native isolated app: both a dormant Codex (GPT-6 Luna) and Claude (Haiku) replied
  `DORMANT_WAKE_OK` to a human `@everyone`; the manually paused original agent stayed paused.
- Runtime regression tests cover scoped/direct/multi-recipient broadcasts, durable reservation,
  blocked claim retention, guards, imported/agent messages, sidechat membership, and a message
  received while a blocked delivery is finishing.
- Live GitHub scoped-tool flow in disposable Project #12: rejected a bare task, created a
  repository Issue, retried with the same task identity, assigned/claimed it, recorded the
  same progress update twice with one retained comment, read its history, and finished with
  evidence. Acceptance Issue [#2](https://github.com/ivg-design/OpenCovibe/issues/2) was closed.
- Live legacy draft flow: unclaimed conversion was rejected; claiming, enriching, conversion,
  exact conversion retry, progress retry and completion all passed while preserving the same
  Project item. Acceptance Issue [#3](https://github.com/ivg-design/OpenCovibe/issues/3) was closed.
- The development fork had Issues disabled. Issues were enabled there for the above tests;
  a repository preflight now reports disabled/unavailable Issues before saving an intent.
- Native 480-task isolated fixture: list pagination reached rows 41–80; search found Task 479;
  the board rendered all four status lanes with ten tasks per lane, retaining their full
  counts. Normal (100%) and enlarged (150%) UI scales were inspected. Cached task description
  remained readable if the remote detail request failed.

No RAV or Nemo Project items were modified by acceptance. Existing real drafts are not
blanket-converted or populated with invented historical evidence; agents upgrade their
own work through the new tool. Remote failures retain local blockers and flag tracking
errors for reconciliation. Exact lost-response recovery is covered by durable intent and
marker checks; deliberately injected live network-loss testing was not performed.

## Verification

- Full frontend verification passed: 1,566 tests in 55 files, lint, formatting, production
  build, and Svelte checks with zero errors (71 existing warnings). Translation checking
  retained its 18 existing warnings.
- Full Rust suite passed: 941 tests, zero failures, five ignored. Final Rust formatting and
  Clippy checks passed with warnings denied.
- Native macOS left-half tiling worked on the updated isolated app; the window was restored
  and the acceptance app closed. All test peers remain paused.
- The updated Local desktop bundle was opened safely after the RAV turn completed. The
  room's original automatic-continuation setting is restored. The new board was observed
  in the actual RAV room: 14 tasks, 13 complete and one blocked, with live ownership,
  evidence summaries, filters, and tracking rules. Claim-release management remains in
  Room settings instead of duplicating the task list below the board.
