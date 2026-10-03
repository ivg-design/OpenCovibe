<script lang="ts">
  import { onMount } from "svelte";
  import { tick, untrack } from "svelte";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import RoomBriefing from "$lib/components/RoomBriefing.svelte";
  import { roomBriefing, readableProtocolOutput } from "$lib/utils/room-presentation";
  import { t } from "$lib/i18n/index.svelte";
  import { roomAgentStateLabel } from "$lib/rooms/state-label";
  import { attachRoomFiles, uploadRoomAttachment, roomClipboardFilePaths } from "$lib/rooms/api";
  import type { Room, RoomAttachment, RoomMessage } from "$lib/rooms/types";
  import RoomAttachmentView from "$lib/components/RoomAttachmentView.svelte";
  import { getTransport } from "$lib/transport";
  import {
    fitRoomAttachmentLimits,
    normalizeDroppedLinks,
    uniqueComposerFiles,
  } from "$lib/utils/room-attachment-composer";
  import { resolveRoomMentionIds } from "$lib/utils/room-mentions";
  import { textareaAutosize } from "$lib/utils/textarea-autosize";
  import type { RoomParticipant } from "$lib/rooms/types";
  import { roomParticipantColor } from "$lib/utils/room-participant-colors";
  let {
    selected,
    actionsDisabled,
    busyAction,
    humanMessage = $bindable(""),
    targetParticipantId = $bindable(""),
    activeSidechatId = $bindable(""),
    attachmentDrafts = $bindable<RoomAttachment[]>([]),
    onBranch,
    onParticipantAction,
    onResumeRoom,
    submitMessage,
  }: {
    selected: Room;
    actionsDisabled: boolean;
    busyAction: string;
    humanMessage?: string;
    targetParticipantId?: string;
    activeSidechatId?: string;
    attachmentDrafts?: RoomAttachment[];
    onParticipantAction: (action: string, participant: RoomParticipant) => void;
    onResumeRoom: () => void;
    onBranch: (sourceMessageId: string, title: string, participantIds: string[]) => Promise<void>;
    submitMessage: (event: SubmitEvent) => Promise<void>;
  } = $props();
  const tr: typeof t = t;
  let feed: HTMLDivElement | undefined;
  let previousFeedKey = "";
  let followingLatest = true;
  let latestHidden = $state(false);
  let feedScrollTop = 0;
  let feedFrame = 0;
  let branchSource = $state<RoomMessage | null>(null);
  let branchTitle = $state("");
  let branchParticipants = $state<string[]>([]);
  let mentionRange = $state<{ start: number; end: number } | null>(null);
  let mentionIndex = $state(0);
  let mentionDismissed = $state(false);
  let attachmentBusy = $state(false);
  let attachmentError = $state("");
  let conversationContext = $derived(`${selected.id}:${activeSidechatId}`);
  let previousContext = "";
  let composer: HTMLTextAreaElement | undefined;
  let messageLimit = $state(50);
  async function showEarlierMessages() {
    if (!feed) return;
    const height = feed.scrollHeight;
    const top = feed.scrollTop;
    followingLatest = false;
    messageLimit += 100;
    await tick();
    if (feed) {
      feed.scrollTop = top + feed.scrollHeight - height;
      feedScrollTop = feed.scrollTop;
    }
  }
  let activeSidechat = $derived(
    selected.sidechats?.find((sidechat) => sidechat.id === activeSidechatId),
  );
  let sourceMessage = $derived(
    selected.messages.find((message) => message.id === activeSidechat?.source_message_id),
  );
  let visibleMessages = $derived(
    selected.messages.filter((message) => (message.sidechat_id ?? "") === activeSidechatId),
  );
  let renderedMessages = $derived(visibleMessages.slice(-messageLimit));
  let visibleParticipants = $derived(
    selected.participants.filter(
      (participant) => !activeSidechat || activeSidechat.participant_ids.includes(participant.id),
    ),
  );
  function explicitRecipients(message: RoomMessage): string[] {
    const targetIds = (message as RoomMessage & { target_participant_ids?: string[] })
      .target_participant_ids;
    return [
      ...new Set([
        ...(targetIds ?? []),
        ...(message.target_participant_id ? [message.target_participant_id] : []),
      ]),
    ];
  }
  function eligibleParticipants(message: RoomMessage): typeof visibleParticipants {
    const recipients = explicitRecipients(message);
    if (!recipients.length) return visibleParticipants;
    return visibleParticipants.filter(
      (participant) =>
        recipients.includes(participant.id) || participant.id === message.participant_id,
    );
  }
  let branchEligibleParticipants = $derived(
    branchSource ? eligibleParticipants(branchSource) : visibleParticipants,
  );
  let mentionOptions = $derived([
    { id: "", name: "everyone" },
    ...visibleParticipants.map((participant) => ({ id: participant.id, name: participant.name })),
  ]);
  let mentionSuggestions = $derived.by(() => {
    if (
      !mentionRange ||
      mentionDismissed ||
      humanMessage[mentionRange.start] !== "@" ||
      mentionRange.end > humanMessage.length
    )
      return [];
    const query = humanMessage.slice(mentionRange.start + 1, mentionRange.end).toLocaleLowerCase();
    return mentionOptions
      .filter((option) => option.name.toLocaleLowerCase().startsWith(query))
      .slice(0, 6);
  });
  let resolvedMentionNames = $derived.by(() => {
    const ids = resolveRoomMentionIds(humanMessage, visibleParticipants);
    if (ids?.length === 0) return ["@everyone"];
    return (ids ?? []).map(
      (id) => visibleParticipants.find((participant) => participant.id === id)?.name ?? id,
    );
  });
  let recipientHint = $derived(
    resolvedMentionNames.length
      ? resolvedMentionNames.join(", ")
      : targetParticipantId
        ? (selected.participants.find((participant) => participant.id === targetParticipantId)
            ?.name ?? targetParticipantId)
        : tr("room_everyone"),
  );
  function startBranch(message: RoomMessage) {
    branchSource = message;
    const briefing = roomBriefing(message.body);
    branchTitle = (briefing?.objective ?? readableProtocolOutput(message.body))
      .replace(/\s+/g, " ")
      .slice(0, 80);
    branchParticipants = eligibleParticipants(message).map((participant) => participant.id);
  }
  function followFeed() {
    if (feed) {
      feed.scrollTop = feed.scrollHeight;
      feedScrollTop = feed.scrollTop;
    }
    followingLatest = true;
    latestHidden = false;
  }
  function trackFeedScroll() {
    if (!feed) return;
    feedScrollTop = feed.scrollTop;
    followingLatest = feed.scrollHeight - feed.scrollTop - feed.clientHeight < 40;
    if (followingLatest) latestHidden = false;
  }
  function keepFeedPosition(node: HTMLDivElement) {
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(feedFrame);
      feedFrame = requestAnimationFrame(() => {
        if (followingLatest) followFeed();
        else node.scrollTop = feedScrollTop;
      });
    });
    observer.observe(node);
    if (node.firstElementChild) observer.observe(node.firstElementChild);
    return {
      destroy() {
        observer.disconnect();
        cancelAnimationFrame(feedFrame);
      },
    };
  }
  let contextGeneration = 0;
  $effect(() => {
    const context = conversationContext;
    if (previousContext && previousContext !== context) {
      contextGeneration++;
      messageLimit = 50;
      attachmentDrafts = [];
      attachmentError = "";
      attachmentBusy = false;
      branchSource = null;
      mentionRange = null;
    }
    previousContext = context;
  });

  function base64(bytes: Uint8Array): string {
    let binary = "";
    for (let i = 0; i < bytes.length; i += 0x8000)
      binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(binary);
  }
  async function addPaths(paths: string[]) {
    const unique = [...new Set(paths)].filter(Boolean);
    if (!unique.length || attachmentBusy || actionsDisabled) return;
    const context = conversationContext;
    const generation = contextGeneration;
    const roomId = selected.id;
    attachmentBusy = true;
    attachmentError = "";
    try {
      const roomAttachments = await attachRoomFiles(roomId, unique);
      if (context === conversationContext && generation === contextGeneration) {
        const fitted = fitRoomAttachmentLimits(attachmentDrafts, roomAttachments);
        if (fitted.rejected.length) {
          attachmentError = tr("room_attachmentLimit");
          return;
        }
        attachmentDrafts = [...attachmentDrafts, ...roomAttachments];
      }
    } catch {
      if (generation === contextGeneration) attachmentError = tr("room_attachmentError");
    } finally {
      if (generation === contextGeneration) attachmentBusy = false;
    }
  }
  async function addFiles(input: Iterable<File>, expectedContext = conversationContext) {
    if (expectedContext !== conversationContext) return;
    const files = uniqueComposerFiles(input);
    if (!files.length || attachmentBusy || actionsDisabled) return;
    const fitted = fitRoomAttachmentLimits(attachmentDrafts, files);
    if (fitted.rejected.length) {
      attachmentError = tr("room_attachmentLimit");
      return;
    }
    if (!fitted.accepted.length) return;
    const context = conversationContext;
    const generation = contextGeneration;
    const roomId = selected.id;
    attachmentBusy = true;
    attachmentError = "";
    try {
      for (const file of fitted.accepted) {
        if (context !== conversationContext || generation !== contextGeneration) return;
        const data = base64(new Uint8Array(await file.arrayBuffer()));
        if (context !== conversationContext || generation !== contextGeneration) return;
        const uploaded = await uploadRoomAttachment(roomId, file.name, data);
        if (context !== conversationContext || generation !== contextGeneration) return;
        attachmentDrafts = [...attachmentDrafts, uploaded];
      }
    } catch {
      if (generation === contextGeneration) attachmentError = tr("room_attachmentError");
    } finally {
      if (generation === contextGeneration) attachmentBusy = false;
    }
  }
  async function chooseAttachments() {
    const context = conversationContext;
    try {
      if (getTransport().isDesktop()) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const paths = await open({ multiple: true, title: tr("room_chooseFiles") });
        if (paths && context === conversationContext)
          await addPaths(Array.isArray(paths) ? paths : [paths]);
      } else {
        const input = document.createElement("input");
        input.type = "file";
        input.multiple = true;
        input.onchange = () => void addFiles(input.files ?? [], context);
        input.click();
      }
    } catch {
      attachmentError = tr("room_attachmentError");
    }
  }
  function onComposerPaste(event: ClipboardEvent) {
    const files = event.clipboardData?.files;
    if (files?.length) {
      event.preventDefault();
      void addFiles(files);
      return;
    }
    if (!getTransport().isDesktop()) return;
    const context = conversationContext;
    const generation = contextGeneration;
    const before = humanMessage;
    const start = composer?.selectionStart ?? before.length;
    const end = composer?.selectionEnd ?? start;
    const pastedText = event.clipboardData?.getData("text/plain") ?? "";
    // Finder/Explorer file copies often contain native file URLs rather than browser Files.
    // Leave ordinary text paste immediate and roll back only its exact inserted text if files arrive.
    void roomClipboardFilePaths()
      .then((paths) => {
        if (!paths.length || context !== conversationContext || generation !== contextGeneration)
          return;
        const expected = before.slice(0, start) + pastedText + before.slice(end);
        if (humanMessage === expected || humanMessage === before) humanMessage = before;
        void addPaths(paths);
      })
      .catch(() => {
        /* Browser text paste still works if the native clipboard is unavailable. */
      });
  }
  function updateMention(event: Event) {
    mentionDismissed = false;
    const node = event.currentTarget as HTMLTextAreaElement;
    const caret = node.selectionStart ?? node.value.length;
    const start = node.value.lastIndexOf("@", caret - 1);
    const preceding = start > 0 ? node.value[start - 1] : " ";
    const precedingAllowed = !preceding || /\s/.test(preceding) || /[([{"',]/.test(preceding);
    const linesBeforeCaret = node.value.slice(0, caret).split(/\r?\n/);
    let fenced = false;
    for (const line of linesBeforeCaret.slice(0, -1))
      if (/^\s*(?:```|~~~)/.test(line)) fenced = !fenced;
    const currentLine = linesBeforeCaret.at(-1) ?? "";
    if (/^\s*(?:```|~~~)/.test(currentLine)) fenced = !fenced;
    let inline = false;
    for (const character of currentLine.slice(0, start < 0 ? 0 : start))
      if (character === "`") inline = !inline;
    if (
      start < 0 ||
      !precedingAllowed ||
      fenced ||
      inline ||
      node.value.slice(start, caret).includes("\n")
    ) {
      mentionRange = null;
      return;
    }
    mentionRange = { start, end: caret };
    mentionIndex = 0;
  }
  function chooseMention(name: string) {
    if (!mentionRange || !composer) return;
    const caret = composer.selectionStart ?? mentionRange.end;
    const replacement = `@${name} `;
    humanMessage = `${humanMessage.slice(0, mentionRange.start)}${replacement}${humanMessage.slice(caret)}`;
    const nextCaret = mentionRange.start + replacement.length;
    mentionRange = null;
    mentionDismissed = true;
    void tick().then(() => {
      composer?.focus();
      composer?.setSelectionRange(nextCaret, nextCaret);
    });
  }
  function onComposerKeydown(event: KeyboardEvent) {
    if (mentionSuggestions.length) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const direction = event.key === "ArrowDown" ? 1 : -1;
        mentionIndex =
          (mentionIndex + direction + mentionSuggestions.length) % mentionSuggestions.length;
        return;
      }
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        chooseMention(mentionSuggestions[mentionIndex].name);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        mentionDismissed = true;
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      if (
        (humanMessage.trim() || attachmentDrafts.length) &&
        !actionsDisabled &&
        !attachmentBusy &&
        event.currentTarget instanceof HTMLTextAreaElement
      )
        event.currentTarget.form?.requestSubmit();
    }
  }
  function onComposerDrop(event: DragEvent) {
    event.preventDefault();
    const files = event.dataTransfer?.files;
    if (files?.length) void addFiles(files);
    const transfer = event.dataTransfer;
    const uriList = transfer?.getData("text/uri-list") ?? "";
    const links = normalizeDroppedLinks(uriList || transfer?.getData("text/plain") || "");
    if (links.length) {
      humanMessage = [humanMessage.trimEnd(), ...links].filter(Boolean).join("\n");
    }
  }
  onMount(() => {
    const transport = getTransport();
    if (!transport.isDesktop()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void transport
      .listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
        if (active && event.paths?.length) void addPaths(event.paths);
      })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      });
    return () => {
      active = false;
      unlisten?.();
    };
  });
  $effect(() => {
    const context = conversationContext;
    const key = `${context}:${visibleMessages.length}:${visibleMessages.at(-1)?.id ?? ""}`;
    untrack(() => {
      if (key === previousFeedKey) return; // A room poll is not a new message.
      const changedContext = !previousFeedKey.startsWith(`${context}:`);
      previousFeedKey = key;
      if (changedContext) followingLatest = true;
      if (followingLatest) void tick().then(followFeed);
      else latestHidden = true;
    });
  });
</script>

<Card class="flex min-h-0 flex-1 flex-col overflow-hidden p-3">
  <div class="max-h-[30%] shrink-0 overflow-y-auto overflow-x-hidden">
    <div class="mb-2 flex shrink-0 flex-wrap items-center justify-between gap-3">
      <label class="min-w-0 flex-1 text-sm">
        <span class="sr-only">{tr("room_conversation")}</span>
        {#if selected.sidechats?.length}
          <select
            class="min-h-9 max-w-full rounded-md border bg-background px-3 text-sm"
            bind:value={activeSidechatId}
            onchange={() => {
              targetParticipantId = humanMessage = "";
              branchSource = null;
            }}
          >
            <option value="">{tr("room_groupChat")}</option>
            {#each selected.sidechats ?? [] as sidechat (sidechat.id)}<option value={sidechat.id}
                >{sidechat.title}</option
              >{/each}
          </select>
        {:else}<span>{tr("room_groupChat")}</span>{/if}
      </label>
      <span class="text-xs text-muted-foreground">{visibleMessages.length}</span>
    </div>
    <div class="mb-2 flex shrink-0 flex-wrap gap-2" aria-label={tr("room_participants")}>
      {#each visibleParticipants as participant (participant.id)}
        <div
          class="flex max-w-full min-w-0 items-center gap-1 rounded-md border px-2 py-1 text-xs"
          style={`border-color: ${roomParticipantColor(selected.participants, participant.id)}`}
        >
          <a
            class="min-w-0 break-words hover:underline"
            href={`/chat?run=${encodeURIComponent(participant.run_id)}`}
            title={tr("room_openSession")}
            ><span
              class="mr-1 inline-block h-2 w-2 rounded-full"
              style={`background: ${roomParticipantColor(selected.participants, participant.id)}`}
            ></span>{participant.name}</a
          >
          <span class="whitespace-nowrap text-muted-foreground"
            >· {roomAgentStateLabel(participant.state)}</span
          >
          <button
            type="button"
            class="shrink-0 rounded px-1 hover:bg-accent"
            disabled={actionsDisabled}
            aria-label={`${participant.paused ? tr("room_resumeParticipant") : tr("room_pauseParticipant")}: ${participant.name}`}
            title={participant.paused ? tr("room_resumeParticipant") : tr("room_pauseParticipant")}
            onclick={() =>
              onParticipantAction(
                participant.paused ? "resume-participant" : "pause-participant",
                participant,
              )}>{participant.paused ? "▶" : "Ⅱ"}</button
          >
        </div>
      {/each}
    </div>
    {#each visibleParticipants.filter((participant) => participant.last_error) as participant (participant.id)}
      <p
        class="mb-2 shrink-0 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-xs text-foreground"
        role="alert"
      >
        <strong>{participant.name}:</strong>
        {participant.last_error}
      </p>
    {/each}
    {#if sourceMessage}<aside class="mb-2 shrink-0 rounded-md border bg-muted/30 p-3 text-xs">
        <p class="font-medium">{tr("room_branchedFrom", { name: sourceMessage.sender })}</p>
        <p class="mt-1 line-clamp-3 whitespace-pre-wrap text-muted-foreground">
          {(
            roomBriefing(sourceMessage.body)?.objective ??
            readableProtocolOutput(sourceMessage.body)
          ).slice(0, 600)}
        </p>
      </aside>{/if}
    {#if branchSource}<form
        class="mb-2 max-h-[35vh] shrink-0 space-y-3 overflow-y-auto rounded-md border p-3"
        onsubmit={async (event) => {
          event.preventDefault();
          await onBranch(branchSource!.id, branchTitle, branchParticipants);
          if (activeSidechatId !== (branchSource?.sidechat_id ?? "")) branchSource = null;
        }}
      >
        <label class="block space-y-1 text-sm"
          ><span>{tr("room_sidechatTitle")}</span><Input bind:value={branchTitle} /></label
        >
        <fieldset class="flex flex-wrap gap-3">
          <legend class="mb-2 text-xs text-muted-foreground">{tr("room_participants")}</legend>
          {#each branchEligibleParticipants as participant (participant.id)}<label
              class="flex items-center gap-2 text-xs"
              ><input type="checkbox" bind:group={branchParticipants} value={participant.id} /><span
                class="min-w-0 break-words">{participant.name}</span
              ></label
            >{/each}
        </fieldset>
        <div class="flex flex-wrap gap-2">
          <Button
            loading={busyAction === "sidechat"}
            disabled={actionsDisabled || !branchTitle.trim() || branchParticipants.length === 0}
            >{tr("room_createSidechat")}</Button
          ><Button
            type="button"
            variant="outline"
            disabled={!!busyAction}
            onclick={() => {
              branchSource = null;
            }}>{tr("common_cancel")}</Button
          >
        </div>
      </form>{/if}
  </div>
  <div class="relative min-h-0 flex-1">
    <div
      class="h-full overflow-y-auto overflow-x-hidden"
      style="overflow-anchor:none"
      bind:this={feed}
      onscroll={trackFeedScroll}
      use:keepFeedPosition
    >
      <div class="space-y-2">
        {#if visibleMessages.length > messageLimit}<button
            type="button"
            class="mb-2 block w-full rounded-md border px-3 py-2 text-xs hover:bg-accent"
            onclick={() => void showEarlierMessages()}
            >{tr("room_loadEarlier", {
              count: String(visibleMessages.length - messageLimit),
            })}</button
          >{/if}
        {#if visibleMessages.length === 0}<p class="text-sm text-muted-foreground">
            {tr("room_noMessages")}
          </p>{/if}{#each renderedMessages as message (message.id)}{@const briefing = roomBriefing(
            message.body,
          )}{@const targetIds = (message as RoomMessage & { target_participant_ids?: string[] })
            .target_participant_ids}
          <article
            class="room-message group {message.participant_id === null
              ? 'room-message-sent'
              : 'room-message-received'} min-w-0 rounded-xl border px-3 py-1.5"
            style={`--room-sender-color: ${
              message.participant_id
                ? roomParticipantColor(selected.participants, message.participant_id)
                : "hsl(var(--primary))"
            }`}
          >
            <div
              class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
            >
              <span class="flex min-w-0 items-center gap-1.5">
                <span
                  class="h-2.5 w-2.5 shrink-0 rounded-full border-2 border-background"
                  style="background:var(--room-sender-color)"
                ></span>
                <span class="min-w-0 break-words font-medium text-foreground">{message.sender}</span
                >
                {#if targetIds && targetIds.length > 1}
                  <span
                    >· {tr("room_directedMessages", {
                      names: targetIds
                        .map((id) => selected.participants.find((p) => p.id === id)?.name ?? id)
                        .join(", "),
                    })}</span
                  >
                {:else if targetIds && targetIds.length === 1}
                  <span
                    >· {tr("room_directedMessage", {
                      name:
                        selected.participants.find((p) => p.id === targetIds[0])?.name ??
                        targetIds[0],
                    })}</span
                  >
                {:else if message.target_participant_id}
                  <span
                    >· {tr("room_directedMessage", {
                      name:
                        selected.participants.find((p) => p.id === message.target_participant_id)
                          ?.name ?? message.target_participant_id,
                    })}</span
                  >
                {/if}
              </span>
              <div class="flex shrink-0 items-center gap-1.5">
                <time>{new Date(message.created_at).toLocaleString()}</time>
                <button
                  type="button"
                  class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                  title={tr("room_branchSidechat")}
                  aria-label={tr("room_branchSidechat")}
                  disabled={actionsDisabled}
                  onclick={() => startBranch(message)}
                  ><svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                    ><circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><circle
                      cx="18"
                      cy="6"
                      r="3"
                    /><path d="M6 9v6M18 9a9 9 0 0 1-9 9" /></svg
                  ></button
                >
              </div>
            </div>
            {#if briefing}<RoomBriefing {briefing} />{:else}<div
                class="prose-chat mt-1 min-w-0 break-words text-sm [overflow-wrap:anywhere]"
              >
                <MarkdownContent
                  text={readableProtocolOutput(message.body)}
                  basePath={selected.repo_path}
                />
              </div>{/if}
            {#if message.attachments?.length}<div class="mt-2 flex min-w-0 flex-col gap-1.5">
                {#each message.attachments as attachment (attachment.id)}
                  <RoomAttachmentView roomId={selected.id} {attachment} />
                {/each}
              </div>{/if}
          </article>{/each}
      </div>
    </div>
    {#if latestHidden}<button
        type="button"
        class="absolute bottom-2 right-2 rounded-md border bg-background px-3 py-1.5 text-xs shadow-sm"
        onclick={followFeed}>{tr("room_jumpLatest")}</button
      >{/if}
  </div>

  <form
    class="room-composer shrink-0 border-t pt-2"
    onsubmit={submitMessage}
    ondragover={(event) => event.preventDefault()}
    ondrop={onComposerDrop}
  >
    {#if selected.paused}<div
        class="mb-2 flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-md border border-primary/60 bg-primary/10 px-3 py-2"
        role="status"
      >
        <p class="min-w-0 flex-1 text-xs text-foreground">{tr("room_chatPausedCompact")}</p>
        <Button type="button" size="sm" disabled={actionsDisabled} onclick={onResumeRoom}
          >{tr("room_resume")}</Button
        >
      </div>{/if}
    {#if attachmentDrafts.length}<div
        class="mb-2 flex min-w-0 flex-wrap gap-1.5"
        aria-label={tr("room_attachments")}
      >
        {#each attachmentDrafts as attachment (attachment.id)}
          <RoomAttachmentView
            roomId={selected.id}
            {attachment}
            onremove={() => {
              attachmentDrafts = attachmentDrafts.filter((item) => item.id !== attachment.id);
              attachmentError = "";
            }}
          />
        {/each}
      </div>{/if}
    {#if attachmentError}<p class="mb-1 text-xs text-destructive" role="alert">
        {attachmentError}
      </p>{/if}
    <p class="mb-1 text-right text-[11px] text-muted-foreground">
      {tr("room_recipientHint", { name: recipientHint })}
    </p>
    {#if mentionSuggestions.length}<div
        class="mb-1 max-h-40 overflow-y-auto rounded-md border bg-background p-1 shadow-sm"
        role="listbox"
        aria-label={tr("room_mentionSuggestions")}
      >
        {#each mentionSuggestions as option, index (`${option.id}:${option.name}`)}
          <button
            type="button"
            role="option"
            aria-selected={index === mentionIndex}
            class="block w-full rounded px-2 py-1 text-left text-sm {index === mentionIndex
              ? 'bg-accent text-foreground'
              : 'text-muted-foreground'}"
            onclick={() => chooseMention(option.name)}
            >{#if !option.id}@{/if}{option.name}</button
          >
        {/each}
      </div>{/if}
    <div class="room-composer-row min-w-0 items-end gap-2">
      <label class="min-w-0 w-28 flex-none space-y-1 text-xs text-muted-foreground"
        ><span class="sr-only">{tr("room_target")}</span><select
          class="h-9 w-full min-w-[90px] rounded-md border bg-background px-2 text-sm text-foreground"
          bind:value={targetParticipantId}
          ><option value="">{tr("room_everyone")}</option
          >{#each visibleParticipants as p (p.id)}<option value={p.id}>{p.name}</option
            >{/each}</select
        ></label
      >
      <div class="room-composer-text min-w-0">
        <label
          ><span class="sr-only">{tr("room_messagePlaceholder")}</span><textarea
            bind:this={composer}
            use:textareaAutosize={{ value: humanMessage, uncapped: true }}
            aria-label={tr("room_messagePlaceholder")}
            class="block min-h-9 w-full min-w-0 resize-none overflow-y-hidden rounded-md border bg-background px-3 py-1.5 text-sm"
            rows="1"
            wrap="soft"
            bind:value={humanMessage}
            placeholder={tr("room_messagePlaceholder")}
            disabled={actionsDisabled}
            onkeydown={onComposerKeydown}
            oninput={updateMention}
            onpaste={onComposerPaste}
          ></textarea></label
        >
      </div>
      <Button
        type="button"
        variant="outline"
        size="sm"
        class="min-h-9 w-9 shrink-0 p-0"
        disabled={actionsDisabled || attachmentBusy || attachmentDrafts.length >= 8}
        onclick={() => void chooseAttachments()}
      >
        {#if attachmentBusy}<svg
            class="mr-1 h-3.5 w-3.5 animate-spin"
            viewBox="0 0 24 24"
            fill="none"
            aria-hidden="true"
            ><circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="4"
            ></circle><path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
            ></path></svg
          >{/if}
        <span class="sr-only">{tr("room_attachFiles")}</span>
        {#if !attachmentBusy}<svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"
            ><path
              d="m21 11-8.5 8.5a6 6 0 0 1-8.5-8.5l9-9a4 4 0 0 1 5.7 5.7l-9 9a2 2 0 0 1-2.8-2.8l8.5-8.5"
            /></svg
          >{/if}
      </Button>
      <Button
        size="sm"
        class="room-send min-h-9 shrink-0 whitespace-nowrap"
        disabled={(!humanMessage.trim() && !attachmentDrafts.length) ||
          actionsDisabled ||
          attachmentBusy}
        loading={busyAction === "message"}
        ><span class="sr-only"
          >{tr(selected.paused ? "room_queueMessage" : "room_sendMessage")}</span
        ><span class="room-send-label" aria-hidden="true"
          >{tr(selected.paused ? "room_queueMessage" : "room_sendMessage")}</span
        ><svg
          class="room-send-icon h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          aria-hidden="true"><path d="m22 2-7 20-4-9-9-4 20-7Z" /><path d="m22 2-11 11" /></svg
        ></Button
      >
    </div>
  </form></Card
>

<style>
  .room-composer {
    container-type: inline-size;
    container-name: room-composer;
  }
  .room-composer-row {
    display: flex;
    flex-wrap: wrap;
  }
  .room-composer-text {
    order: -1;
    flex-basis: 100%;
  }
  .room-composer-row > label {
    max-width: 100%;
  }
  .room-send-icon {
    display: none;
  }
  @container room-composer (max-width: 20rem) {
    .room-send-label {
      display: none;
    }
    .room-send-icon {
      display: block;
    }
    .room-composer-row :global(.room-send) {
      width: 2.25rem;
      padding-inline: 0;
    }
  }
  .room-message {
    border-color: color-mix(in srgb, var(--room-sender-color) 60%, hsl(var(--border)));
    border-left: 3px solid var(--room-sender-color);
    background: color-mix(in srgb, var(--room-sender-color) 12%, transparent);
  }
  .room-message :global(.prose-chat :is(p, ul, ol, blockquote, pre)) {
    margin-block: 0.25rem;
    line-height: 1.4;
  }
</style>
