<script lang="ts">
  import { untrack } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import { textareaAutosize } from "$lib/utils/textarea-autosize";
  import { t } from "$lib/i18n/index.svelte";
  import { filterRoomRequests, isOpenRoomRequest, needsHumanAnswer } from "$lib/rooms/requests";
  import type { Room, RoomRequest } from "$lib/rooms/types";
  import { identityName } from "$lib/stores/identity.svelte";
  import { isRequestReplyShortcut, requestReplyApproval } from "$lib/rooms/request-reply";

  let {
    room,
    disabled,
    busyAction,
    onResolve,
    onApproveAgent,
    onCloseRequest,
    onArchive,
    onClosePanel,
    revealToken = "",
  }: {
    room: Room;
    disabled: boolean;
    busyAction: string;
    onResolve: (request: RoomRequest, approve: boolean, response: string) => void;
    onApproveAgent: (request: RoomRequest) => void;
    onCloseRequest: (request: RoomRequest, reason: string) => void;
    onArchive: (requestIds: string[], archived: boolean) => void;
    onClosePanel: () => void;
    revealToken?: string;
  } = $props();
  let replies = $state<Record<string, string>>({});
  let selectedId = $state("");
  let query = $state("");
  let scope = $state<"open" | "attention" | "waiting" | "history" | "archived" | "all">(
    "attention",
  );
  let kind = $state("all");
  const requestKinds = ["decision", "review", "agent", "completion"] as const;
  $effect(() => {
    const token = revealToken;
    if (!token) return;
    untrack(() => {
      scope = "attention";
      kind = "all";
      query = "";
      selectedId = token.split(":")[0];
    });
  });
  const nameFor = (id: string | null) =>
    id === "Human"
      ? identityName()
      : id
        ? (room.participants.find((p) => p.id === id)?.name ?? t("room_requestsUnknownPerson"))
        : t("room_requestsUnassigned");
  const openCount = $derived(room.requests.filter(isOpenRoomRequest).length);
  const humanCount = $derived(room.requests.filter(needsHumanAnswer).length);
  const resolvedCount = $derived(
    room.requests.filter((r) => !r.archived && !isOpenRoomRequest(r)).length,
  );
  const filtered = $derived(
    filterRoomRequests(room.requests, scope, query, room.participants, kind),
  );
  const selectedRequest = $derived(
    filtered.find((r) => r.id === selectedId) ?? filtered[0] ?? null,
  );
  $effect(() => {
    if (selectedRequest && selectedId !== selectedRequest.id) selectedId = selectedRequest.id;
  });
  function resetFilter() {
    selectedId = "";
  }
  const busy = (request: RoomRequest) =>
    disabled || busyAction === `request:${request.id}` || !!busyAction;
  const needsReason = (request: RoomRequest) =>
    request.kind === "agent" || request.kind === "review" || request.kind === "completion";
  function resolve(request: RoomRequest, approve: boolean) {
    const response = replies[request.id]?.trim() ?? "";
    if (!response && (!approve || needsReason(request))) return;
    onResolve(request, approve, response);
  }
  function replyKeydown(event: KeyboardEvent, request: RoomRequest) {
    if (!isRequestReplyShortcut(event)) return;
    event.preventDefault();
    const approval = requestReplyApproval(room, request, replies[request.id] ?? "", busy(request));
    if (approval !== null) resolve(request, approval);
  }
</script>

<section
  class="request-panel flex h-full min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border bg-background"
  aria-label={t("room_requestsTitle")}
>
  <header class="shrink-0 space-y-2 border-b p-3">
    <div class="flex min-w-0 items-start gap-2">
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1">
        <h2 class="text-sm font-semibold">{t("room_requestsTitle")} · {openCount}</h2>
        <span
          class="rounded-md px-2 py-1 text-xs {humanCount
            ? 'bg-amber-500/15 text-amber-600 dark:text-amber-400'
            : 'text-muted-foreground'}"
          role="status"
        >
          {humanCount
            ? t("room_requestsNeedYou", { count: String(humanCount) })
            : t("room_requestsNoAnswerNeeded")}
        </span>
      </div>
      <button
        type="button"
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-input bg-background text-muted-foreground hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={t("room_requestsHide")}
        title={t("room_requestsHide")}
        onclick={onClosePanel}
        ><svg
          aria-hidden="true"
          class="h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"><path d="m6 6 12 12M18 6 6 18" /></svg
        ></button
      >
    </div>
    <div class="grid min-w-0 grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] gap-2">
      <label
        ><span class="sr-only">{t("room_requestsShow")}</span><select
          class="min-h-9 w-full max-w-full rounded-md border bg-background px-2 text-xs"
          bind:value={scope}
          onchange={resetFilter}
        >
          <option value="open">{t("room_requestsOpen")}</option><option value="history"
            >{t("room_requestsResolved")}</option
          ><option value="attention">{t("room_requestsYourAnswer")}</option><option value="waiting"
            >{t("room_requestsWaiting")}</option
          ><option value="archived">{t("room_requestsArchived")}</option><option value="all"
            >{t("room_requestsAll")}</option
          >
        </select></label
      >
    </div>
    <details class="text-xs">
      <summary class="cursor-pointer text-muted-foreground"
        >{t("room_requestsFiltersCleanup")}{#if query || kind !== "all"}
          · {t("room_requestsFiltered")}{/if}</summary
      >
      <div class="mt-2 space-y-2">
        <label class="min-w-0 flex-1"
          ><span class="sr-only">{t("room_requestsSearch")}</span><Input
            bind:value={query}
            oninput={resetFilter}
            placeholder={t("room_requestsSearch")}
          /></label
        >
        <div
          class="grid min-w-0 grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] items-center gap-2"
        >
          <label class="min-w-0 flex-1"
            ><span class="sr-only">{t("room_requestsKind")}</span><select
              class="min-h-8 w-full rounded border bg-background px-2 text-xs"
              bind:value={kind}
              onchange={resetFilter}
              ><option value="all">{t("room_requestsAllKinds")}</option
              >{#each requestKinds as value}<option {value}>{t(`room_requestKind_${value}`)}</option
                >{/each}</select
            ></label
          >
          <Button
            size="sm"
            variant="outline"
            disabled={disabled || !!busyAction || !resolvedCount}
            onclick={() => onArchive([], true)}>{t("room_requestsArchiveResolved")}</Button
          >
        </div>
      </div>
    </details>
  </header>
  <div class="request-context min-h-0 min-w-0 flex-1 overflow-hidden">
    <div
      class="request-list min-h-0 overflow-y-auto overflow-x-hidden border-b"
      aria-label={t("room_requestsList")}
    >
      {#each filtered as request (request.id)}
        <button
          type="button"
          class="block w-full min-w-0 border-b px-3 py-2 text-left text-xs hover:bg-accent focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring {selectedRequest?.id ===
          request.id
            ? 'bg-primary/10'
            : ''}"
          aria-pressed={selectedRequest?.id === request.id}
          onclick={() => {
            selectedId = request.id;
          }}
        >
          <span class="block break-words font-medium">{request.title}</span>
          <span class="mt-1 flex flex-wrap gap-x-2 gap-y-1 text-muted-foreground">
            <span
              >{t(`room_requestKind_${request.kind}`)} · {t(
                `room_requestStatus_${request.status}`,
              )}</span
            >
            <span
              >{nameFor(request.requester_id)}{#if request.reviewer_id}
                → {nameFor(request.reviewer_id)}{/if}</span
            >
            {#if needsHumanAnswer(request)}<span
                class="font-medium text-amber-600 dark:text-amber-400"
                >{t("room_requestsYourAnswer")}</span
              >{/if}
          </span>
        </button>
      {/each}
      {#if !filtered.length}<p class="p-3 text-xs text-muted-foreground">
          {query ? t("room_requestsNoMatches") : t("room_requestsEmpty")}
        </p>{/if}
    </div>
    {#if selectedRequest}
      {@const request = selectedRequest}
      <div
        class="request-details min-h-0 overflow-y-auto overflow-x-hidden space-y-2 p-3"
        aria-label={t("room_requestsDetails")}
      >
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0">
            <h3 class="break-words font-medium">{request.title}</h3>
            <p class="mt-1 break-words text-xs text-muted-foreground">
              {t(`room_requestKind_${request.kind}`)} · {t(`room_requestStatus_${request.status}`)} ·
              {t("room_requestsRequester", { name: nameFor(request.requester_id) })}
            </p>
          </div>
          <time class="shrink-0 whitespace-nowrap text-xs text-muted-foreground"
            >{new Date(request.created_at).toLocaleString()}</time
          >
        </div>
        <div class="text-sm"><MarkdownContent text={request.body} /></div>
        {#if request.task_id}<p class="break-all text-xs text-muted-foreground">
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
            <p class="mt-1 whitespace-pre-wrap break-words">{request.response}</p>
          </div>{/if}
        {#if request.kind === "completion" && request.status === "verified"}<p
            class="text-xs text-muted-foreground"
          >
            {room.paused ? t("room_requestsAcceptPausedHelp") : t("room_requestsPauseFirst")}
          </p>{/if}
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
      </div>
    {:else}<p class="min-h-0 overflow-y-auto p-3 text-xs text-muted-foreground">
        {t("room_requestsChoose")}
      </p>{/if}
  </div>
  {#if selectedRequest}
    {#key selectedRequest.id}
      {@const request = selectedRequest}
      {@const isBusy = busy(request)}
      {#if !room.archived && ((request.kind === "agent" && ["pending", "creating"].includes(request.status)) || (request.kind === "decision" && request.status === "pending") || (["review", "completion"].includes(request.kind) && ["pending", "verified", "changes_requested"].includes(request.status)))}
        {@const answerLabel =
          request.kind === "decision"
            ? t("room_requestsAnswer")
            : request.kind === "review"
              ? t("room_requestsCancelReason")
              : request.kind === "completion" && request.status === "verified"
                ? t("room_requestsAcceptanceNote")
                : t("room_requestsRejectReason")}
        <div class="request-answer shrink-0 space-y-2 border-t p-2">
          {#if request.kind === "review" || request.kind === "completion"}
            <p class="text-xs text-muted-foreground">
              {request.status === "verified"
                ? t("room_requestsVerifiedBy", { name: nameFor(request.reviewer_id) })
                : request.status === "changes_requested"
                  ? t("room_requestsReviewChangesRequested")
                  : request.kind === "review"
                    ? t("room_requestsAwaitReviewer")
                    : t("room_requestsAwaitCompletionReview")}
            </p>
          {/if}

          <label class="block w-full min-w-0">
            <span class="sr-only">{answerLabel}</span>
            <textarea
              class="block min-h-[60px] w-full resize-none overflow-hidden rounded-md border border-input bg-transparent px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              aria-label={answerLabel}
              placeholder={answerLabel}
              rows="2"
              onkeydown={(event) => replyKeydown(event, request)}
              bind:value={() => replies[request.id] ?? "", (value) => (replies[request.id] = value)}
              use:textareaAutosize={{ value: replies[request.id] ?? "", uncapped: true }}
            ></textarea>
          </label>
          <div class="flex flex-wrap items-center gap-2">
            {#if request.kind === "decision"}
              <Button
                size="sm"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, true)}>{t("room_requestsSendAnswer")}</Button
              >
              <Button
                size="sm"
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}>{t("room_requestsDecline")}</Button
              >
            {:else if request.kind === "agent"}
              {@const peerAlreadyExists =
                !!request.approved_participant_id ||
                room.participants.some((participant) => participant.id === request.id)}
              <Button
                size="sm"
                disabled={isBusy}
                loading={busyAction === `request:${request.id}`}
                onclick={() => onApproveAgent(request)}
                >{request.status === "creating"
                  ? t("room_requestsRetryAgent")
                  : t("room_requestsApproveAgent")}</Button
              >
              <Button
                size="sm"
                variant="outline"
                disabled={isBusy || peerAlreadyExists || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}>{t("room_requestsReject")}</Button
              >
            {:else if request.kind === "review"}
              <Button
                size="sm"
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}>{t("room_requestsCancelReview")}</Button
              >
            {:else if request.kind === "completion"}
              {#if request.status === "verified"}
                <Button
                  size="sm"
                  disabled={isBusy || !room.paused || !replies[request.id]?.trim()}
                  onclick={() => resolve(request, true)}
                  >{t("room_requestsAcceptCompletion")}</Button
                >
              {/if}
              <Button
                size="sm"
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => resolve(request, false)}
                >{request.status === "changes_requested"
                  ? t("room_requestsCancelCompletion")
                  : t("room_requestsReject")}</Button
              >
            {/if}
            {#if request.status !== "creating"}<Button
                size="sm"
                variant="outline"
                disabled={isBusy || !replies[request.id]?.trim()}
                onclick={() => onCloseRequest(request, replies[request.id].trim())}
                >{t("room_requestsCloseObsolete")}</Button
              >{/if}
          </div>
          {#if request.kind === "agent" && (!!request.approved_participant_id || room.participants.some((participant) => participant.id === request.id))}<p
              class="text-xs text-muted-foreground"
            >
              {t("room_requestsAgentRetryOnly")}
            </p>{/if}
        </div>
      {:else if !isOpenRoomRequest(request)}
        <div class="shrink-0 border-t p-2">
          <Button
            size="sm"
            variant="outline"
            disabled={isBusy}
            onclick={() => onArchive([request.id], !request.archived)}
            >{request.archived ? t("room_requestsRestore") : t("room_requestsArchive")}</Button
          >
        </div>
      {/if}
    {/key}
  {/if}
</section>

<style>
  .request-context {
    display: grid;
    grid-template-rows: minmax(2.5rem, 25%) minmax(0, 1fr);
  }
  .request-list {
    scrollbar-gutter: stable;
  }

  .request-details {
    overflow-wrap: break-word;
  }
  .request-details :global(pre) {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .request-details :global(table) {
    display: block;
    max-width: 100%;
    overflow-wrap: anywhere;
  }
</style>
