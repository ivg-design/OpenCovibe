# Peer-addressed dormant wake acceptance — 2026-10-01

## Report and cause

In the actual RAV room, the Lead had an unread assigned review request addressed to Claude,
while Claude was automatically paused in `no_progress`. The previous wake predicate only
accepted fresh human input, leaving the request undelivered.

Fresh directed peer messages, explicit named mentions, peer `@everyone`, and assigned
review requests now revive automatically blocked/no-progress/completed recipients.
Recipient matching excludes the sender and respects sidechat membership. Ordinary peer
progress broadcasts and imported session output do not revive dormant peers. Explicit
human pauses, room pauses, permission waits, provider failures, unresolved deliveries,
and enabled turn limits remain guarded. A queued address is reconsidered when a prior
turn finishes and after restart; consumed messages are not delivered again.

## Startup boundary

The first isolated live test woke both providers. Codex replied, but Claude's last allowed
turn could not use the review-response tool: its startup `spawning -> idle -> running`
sequence cleared the delivery on the first idle event. Delivery state now durably records
whether the turn actually started. Startup idle preserves the intent; running marks it
started, and terminal idle then finishes it. Startup errors still fail visibly. This
preserves last-turn tool access and avoids premature completion or another delivery.
Older saved in-flight deliveries retain their previous completion behavior when the
new startup marker is absent, so upgrading does not strand their final idle event.

## Verification

- Full Rust suite: 950 passed, zero failed, five ignored.
- Rust formatting and Clippy passed with warnings denied.
- Regression coverage includes direct IDs, single/multiple named mentions, peer Everyone,
  review requests, blocked-claim retention, self exclusion, read-once persistence,
  scoped sidechats, input during an in-flight turn, manual/provider/budget guards,
  imported history, and startup idle/running/completion including a final allowed turn.
- Corrected native isolated rerun passed with actual GPT-6 Luna and Claude Haiku sessions:
  an ordinary broadcast kept both dormant; a named peer message woke only Codex; an
  assigned review request woke Claude. Both replied `PEER_WAKE_OK`. Claude successfully
  saved its response through the room tool on its final allowed turn. The non-appointed
  Codex peer was correctly denied review-response authority. Test peers were paused and
  the isolated acceptance app closed afterward.
- The updated Local bundle was opened after verifying no RAV turn was in flight. Original
  automatic continuation was restored. Without a new test message or manual resume,
  Claude became busy and acknowledged the existing `02c1386e` review at 10:51:21 UTC:
  “Review `02c1386e` of the Lead's integration branch is assigned to me, so I'm starting
  on it.” Its next message reported inspecting the selection-contract diffs. The Codex
  delegate remained correctly blocked on its human scope choice. This proves delivery
  recovery in the actual room. Claude subsequently returned a changes-requested review,
  posted a tracked follow-up (Issue #26), and addressed the Lead. The Lead acknowledged
  the review and began handling the follow-up. The product fix and its acceptance remain
  separate RAV work; no claim is made here about completing that fix.
