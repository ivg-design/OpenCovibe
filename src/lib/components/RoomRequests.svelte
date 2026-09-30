<script lang="ts">
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import Textarea from "$lib/components/Textarea.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { Room, RoomRequest } from "$lib/rooms/types";

  let {
    room,
    disabled,
    busyAction,
    onResolve,
    onApproveAgent,
  }: {
    room: Room;
    disabled: boolean;
    busyAction: string;
    onResolve: (request: RoomRequest, approve: boolean, response: string) => void;
    onApproveAgent: (request: RoomRequest) => void;
  } = $props();
  let replies = $state<Record<string, string>>({});
  let historyOpen = $state(false);
  const openStatuses = new Set(["pending", "creating", "changes_requested", "verified"]);
  const nameFor = (id: string | null) =>
    id === "Human"
      ? t("room_requestsHuman")
      : id
        ? (room.participants.find((p) => p.id === id)?.name ?? t("room_requestsUnknownPerson"))
        : t("room_requestsUnassigned");
  const pending = $derived(
    room.requests
      .filter((request) => openStatuses.has(request.status))
      .sort((a, b) => {
        const rank = (status: string) =>
          status === "pending"
            ? 0
            : status === "verified"
              ? 1
              : status === "changes_requested"
                ? 2
                : 3;
        return rank(a.status) - rank(b.status) || b.created_at.localeCompare(a.created_at);
      }),
  );
  const history = $derived(
    room.requests
      .filter((request) => !openStatuses.has(request.status))
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
  );
  const busy = (request: RoomRequest) =>
    disabled || busyAction === `request:${request.id}` || !!busyAction;
  const needsReason = (request: RoomRequest) =>
    request.kind === "agent" || request.kind === "review" || request.kind === "completion";
  function resolve(request: RoomRequest, approve: boolean) {
    const response = replies[request.id]?.trim() ?? "";
    if (!response && (!approve || needsReason(request))) return;
    onResolve(request, approve, response);
  }
</script>

<section class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-base font-semibold">{t("room_requestsTitle")}</h2>
    <span class="text-xs text-muted-foreground">{room.requests.length}</span>
  </div>
  {#if pending.length === 0}<Card variant="subtle" class="p-4 text-sm text-muted-foreground"
      >{t("room_requestsEmpty")}</Card
    >{/if}
  <div class="grid gap-3">
    {#each pending as request (request.id)}
      {@const isBusy = busy(request)}
      <Card class="space-y-3 p-4">
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0">
            <h3 class="font-medium">{request.title}</h3>
            <p class="mt-1 text-xs text-muted-foreground">
              {t(`room_requestKind_${request.kind}`)} · {t(`room_requestStatus_${request.status}`)} ·
              {t("room_requestsRequester", { name: nameFor(request.requester_id) })}
            </p>
          </div>
          <time class="text-xs text-muted-foreground"
            >{new Date(request.created_at).toLocaleString()}</time
          >
        </div>
        <div class="text-sm"><MarkdownContent text={request.body} /></div>
        {#if request.task_id}<p class="text-xs text-muted-foreground">
            {t("room_requestsTask", { task: request.task_id })}
          </p>{/if}
        {#if request.evidence}<div class="rounded-md border bg-muted/30 p-3">
            <p class="mb-1 text-xs font-medium">{t("room_requestsEvidence")}</p>
            <MarkdownContent text={request.evidence} />
          </div>{/if}
        {#if request.options.length}<div class="space-y-1 text-sm">
            <p class="text-xs font-medium text-muted-foreground">{t("room_requestsOptions")}</p>
            <ul class="list-inside list-disc">
              {#each request.options as option}<li>{option}</li>{/each}
            </ul>
          </div>{/if}
        {#if request.brief}<div class="rounded-md bg-muted/40 p-3">
            <p class="mb-1 text-xs font-medium">{t("room_requestsBrief")}</p>
            <MarkdownContent text={request.brief} />
          </div>{/if}
        {#if request.proposal}<div class="rounded-md border p-3 text-sm">
            <p class="mb-1 text-xs font-medium text-muted-foreground">
              {t("room_requestsProposedPeer")}
            </p>
            <p>
              {request.proposal.name} · {request.proposal.provider} · {request.proposal.model ??
                t("room_defaultModel")} · {request.proposal.effort ?? t("room_defaultEffort")}
            </p>
            <p class="text-xs text-muted-foreground">
              {request.proposal.use_worktree
                ? t("room_useWorktree")
                : t("room_requestsSharedWorkspace")} · {t("room_maxTurns")}: {request.proposal
                .max_turns || t("room_noTurnLimit")}
            </p>
            <p class="mt-2 text-xs text-muted-foreground">{t("room_requestsPausedPeerHelp")}</p>
          </div>{/if}
        {#if request.approved_participant_id && room.participants.find((p) => p.id === request.approved_participant_id)?.brief}
          <div class="rounded-md bg-muted/40 p-3">
            <p class="mb-1 text-xs font-medium">{t("room_requestsPeerBrief")}</p>
            <MarkdownContent
              text={room.participants.find((p) => p.id === request.approved_participant_id)
                ?.brief ?? ""}
            />
          </div>
        {/if}
        {#if request.reviewer_id}<p class="text-xs text-muted-foreground">
            {t("room_requestsReviewer", { name: nameFor(request.reviewer_id) })}
          </p>{/if}
        {#if request.response && (!request.review_response || request.response !== request.review_response || request.resolved_by !== request.reviewed_by)}<div
            class="rounded-md bg-muted/40 p-3 text-sm"
          >
            <span class="text-xs font-medium"
              >{t("room_requestsPriorResponse", { name: nameFor(request.resolved_by) })}</span
            >
            <p class="mt-1 whitespace-pre-wrap">{request.response}</p>
          </div>{/if}
        {#if request.review_response}<div
            class="rounded-md border border-primary/20 bg-primary/5 p-3 text-sm"
          >
            <div class="flex flex-wrap justify-between gap-2 text-xs font-medium">
              <span
                >{t("room_requestsPeerReviewResponse", {
                  name: nameFor(request.reviewed_by ?? request.reviewer_id),
                })}</span
              >
              {#if request.reviewed_at}<time>{new Date(request.reviewed_at).toLocaleString()}</time
                >{/if}
            </div>
            <div class="mt-1"><MarkdownContent text={request.review_response} /></div>
          </div>{/if}

        {#if !room.archived}
          {#if request.kind === "agent" && ["pending", "creating"].includes(request.status)}
            {@const peerAlreadyExists =
              !!request.approved_participant_id ||
              room.participants.some((participant) => participant.id === request.id)}
            <div class="flex flex-wrap gap-2">
              <Button
                disabled={isBusy}
                loading={busyAction === `request:${request.id}`}
                onclick={() => onApproveAgent(request)}
                >{request.status === "creating"
                  ? t("room_requestsRetryAgent")
                  : t("room_requestsApproveAgent")}</Button
              >
              <div class="flex flex-wrap min-w-0 flex-1 gap-2">
                <label class="min-w-0 flex-1"
                  ><span class="sr-only">{t("room_requestsRejectReason")}</span><Input
                    bind:value={
                      () => replies[request.id] ?? "", (value) => (replies[request.id] = value)
                    }
                    placeholder={t("room_requestsRejectReason")}
                  /></label
                ><Button
                  variant="outline"
                  disabled={isBusy || peerAlreadyExists || !replies[request.id]?.trim()}
                  onclick={() => resolve(request, false)}>{t("room_requestsReject")}</Button
                >
              </div>
            </div>
            {#if peerAlreadyExists}<p class="text-xs text-muted-foreground">
                {t("room_requestsAgentRetryOnly")}
              </p>{/if}
          {:else if request.kind === "decision" && request.status === "pending"}
            <div class="flex flex-wrap gap-2">
              <label class="min-w-0 flex-1"
                ><span class="sr-only">{t("room_requestsAnswer")}</span><Textarea
                  bind:value={
                    () => replies[request.id] ?? "", (value) => (replies[request.id] = value)
                  }
                  rows={2}
                  placeholder={t("room_requestsAnswer")}
                /></label
              ><Button
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, true)}>{t("room_requestsSendAnswer")}</Button
              ><Button
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}>{t("room_requestsDecline")}</Button
              >
            </div>
          {:else if request.kind === "review" && ["pending", "changes_requested"].includes(request.status)}
            {#if request.status === "pending"}<p class="text-sm text-muted-foreground">
                {t("room_requestsAwaitReviewer")}
              </p>{/if}
            {#if request.status === "changes_requested"}<p class="text-sm text-muted-foreground">
                {t("room_requestsReviewChangesRequested")}
              </p>{/if}
            <div class="flex flex-wrap gap-2">
              <label class="min-w-0 flex-1"
                ><span class="sr-only">{t("room_requestsCancelReason")}</span><Input
                  bind:value={
                    () => replies[request.id] ?? "", (value) => (replies[request.id] = value)
                  }
                  placeholder={t("room_requestsCancelReason")}
                /></label
              ><Button
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}>{t("room_requestsCancelReview")}</Button
              >
            </div>
          {:else if request.kind === "completion"}
            {#if request.status === "pending"}<p class="text-sm text-muted-foreground">
                {t("room_requestsAwaitCompletionReview")}
              </p>{/if}
            {#if request.status === "changes_requested"}<p class="text-sm text-muted-foreground">
                {t("room_requestsReviewChangesRequested")}
              </p>{/if}
            {#if request.status === "verified"}<div class="space-y-2">
                <p class="text-sm text-muted-foreground">
                  {t("room_requestsVerifiedBy", { name: nameFor(request.reviewer_id) })}
                </p>
                <label class="block"
                  ><span class="sr-only">{t("room_requestsAcceptanceNote")}</span><Input
                    bind:value={
                      () => replies[request.id] ?? "", (value) => (replies[request.id] = value)
                    }
                    placeholder={t("room_requestsAcceptanceNote")}
                  /></label
                >
                <p class="text-xs text-muted-foreground">
                  {room.paused ? t("room_requestsAcceptPausedHelp") : t("room_requestsPauseFirst")}
                </p>
                <Button
                  disabled={isBusy || !room.paused || !replies[request.id]?.trim()}
                  onclick={() => resolve(request, true)}
                  >{t("room_requestsAcceptCompletion")}</Button
                >
              </div>{/if}
            {#if ["pending", "verified", "changes_requested"].includes(request.status)}<div
                class="flex flex-wrap gap-2"
              >
                <label class="min-w-0 flex-1"
                  ><span class="sr-only">{t("room_requestsRejectReason")}</span><Input
                    bind:value={
                      () => replies[request.id] ?? "", (value) => (replies[request.id] = value)
                    }
                    placeholder={t("room_requestsRejectReason")}
                  /></label
                ><Button
                  variant="outline"
                  disabled={isBusy || !replies[request.id]?.trim()}
                  onclick={() => resolve(request, false)}
                  >{request.status === "changes_requested"
                    ? t("room_requestsCancelCompletion")
                    : t("room_requestsReject")}</Button
                >
              </div>{/if}
          {/if}
        {/if}
      </Card>
    {/each}
  </div>
  {#if history.length}<details bind:open={historyOpen} class="rounded-lg border bg-card">
      <summary class="cursor-pointer px-4 py-3 text-sm font-medium"
        >{t("room_requestsHistory", { count: String(history.length) })}</summary
      >
      <div class="space-y-2 border-t p-3">
        {#each history as request (request.id)}<article class="rounded-md border p-3 text-sm">
            <div class="flex flex-wrap justify-between gap-2">
              <h3 class="font-medium">{request.title}</h3>
              <span class="text-xs text-muted-foreground"
                >{t(`room_requestKind_${request.kind}`)} · {t(
                  `room_requestStatus_${request.status}`,
                )}</span
              >
            </div>
            <p class="mt-1 text-xs text-muted-foreground">
              {t("room_requestsRequester", { name: nameFor(request.requester_id) })} · {new Date(
                request.updated_at,
              ).toLocaleString()}
            </p>
            <div class="mt-2"><MarkdownContent text={request.body} /></div>
            {#if request.task_id}<p class="mt-2 text-xs text-muted-foreground">
                {t("room_requestsTask", { task: request.task_id })}
              </p>{/if}
            {#if request.evidence}<div class="mt-2">
                <p class="text-xs font-medium">{t("room_requestsEvidence")}</p>
                <MarkdownContent text={request.evidence} />
              </div>{/if}{#if request.response && (!request.review_response || request.response !== request.review_response || request.resolved_by !== request.reviewed_by)}<p
                class="mt-2 whitespace-pre-wrap text-muted-foreground"
              >
                {t("room_requestsPriorResponse", { name: nameFor(request.resolved_by) })}: {request.response}
              </p>{/if}{#if request.review_response}<div
                class="mt-2 rounded-md border border-primary/20 bg-primary/5 p-2 text-sm"
              >
                <div class="flex flex-wrap justify-between gap-2 text-xs font-medium">
                  <span
                    >{t("room_requestsPeerReviewResponse", {
                      name: nameFor(request.reviewed_by ?? request.reviewer_id),
                    })}</span
                  >
                  {#if request.reviewed_at}<time
                      >{new Date(request.reviewed_at).toLocaleString()}</time
                    >{/if}
                </div>
                <div class="mt-1"><MarkdownContent text={request.review_response} /></div>
              </div>{/if}{#if request.proposal}<p class="mt-2 text-xs">
                {t("room_requestsProposedPeer")}: {request.proposal.name} · {request.proposal
                  .provider} · {request.proposal.model ?? t("room_defaultModel")} · {request
                  .proposal.effort ?? t("room_defaultEffort")} · {request.proposal.use_worktree
                  ? t("room_useWorktree")
                  : t("room_requestsSharedWorkspace")} · {t("room_maxTurns")}: {request.proposal
                  .max_turns || t("room_noTurnLimit")}
              </p>{/if}
            {#if request.options.length}<div class="mt-2 text-xs">
                <p class="font-medium">{t("room_requestsOptions")}</p>
                <ul class="list-inside list-disc">
                  {#each request.options as option}<li>{option}</li>{/each}
                </ul>
              </div>{/if}
            {#if request.brief}<div class="mt-2">
                <p class="text-xs font-medium">{t("room_requestsBrief")}</p>
                <MarkdownContent text={request.brief} />
              </div>{/if}
            {#if request.reviewer_id}<p class="mt-2 text-xs text-muted-foreground">
                {t("room_requestsReviewer", { name: nameFor(request.reviewer_id) })}
              </p>{/if}
            {#if request.approved_participant_id && room.participants.find((p) => p.id === request.approved_participant_id)?.brief}
              <div class="mt-2">
                <p class="text-xs font-medium">{t("room_requestsPeerBrief")}</p>
                <MarkdownContent
                  text={room.participants.find((p) => p.id === request.approved_participant_id)
                    ?.brief ?? ""}
                />
              </div>
            {/if}
          </article>{/each}
      </div>
    </details>{/if}
</section>
