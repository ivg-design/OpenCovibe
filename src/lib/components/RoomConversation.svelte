<script lang="ts">
  import { tick, untrack } from "svelte";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import RoomBriefing from "$lib/components/RoomBriefing.svelte";
  import { roomBriefing, readableProtocolOutput } from "$lib/utils/room-presentation";
  import { t } from "$lib/i18n/index.svelte";
  import { roomAgentStateLabel } from "$lib/rooms/state-label";
  import type { Room, RoomMessage } from "$lib/rooms/types";
  let {
    selected,
    actionsDisabled,
    busyAction,
    humanMessage = $bindable(""),
    targetParticipantId = $bindable(""),
    activeSidechatId = $bindable(""),
    onBranch,
    submitMessage,
  }: {
    selected: Room;
    actionsDisabled: boolean;
    busyAction: string;
    humanMessage?: string;
    targetParticipantId?: string;
    activeSidechatId?: string;
    onBranch: (sourceMessageId: string, title: string, participantIds: string[]) => Promise<void>;
    submitMessage: (event: SubmitEvent) => Promise<void>;
  } = $props();
  const tr: typeof t = t;
  let feed: HTMLDivElement | undefined;
  let previousRoom = "";
  let previousSidechat = "";
  let branchSource = $state<RoomMessage | null>(null);
  let branchTitle = $state("");
  let branchParticipants = $state<string[]>([]);
  let activeSidechat = $derived(
    selected.sidechats?.find((sidechat) => sidechat.id === activeSidechatId),
  );
  let sourceMessage = $derived(
    selected.messages.find((message) => message.id === activeSidechat?.source_message_id),
  );
  let visibleMessages = $derived(
    selected.messages.filter((message) => (message.sidechat_id ?? "") === activeSidechatId),
  );
  let visibleParticipants = $derived(
    selected.participants.filter(
      (participant) => !activeSidechat || activeSidechat.participant_ids.includes(participant.id),
    ),
  );
  let branchEligibleParticipants = $derived(
    visibleParticipants.filter(
      (participant) =>
        !branchSource?.target_participant_id ||
        participant.id === branchSource.target_participant_id ||
        participant.id === branchSource.participant_id,
    ),
  );
  function startBranch(message: RoomMessage) {
    branchSource = message;
    const briefing = roomBriefing(message.body);
    branchTitle = (briefing?.objective ?? readableProtocolOutput(message.body))
      .replace(/\s+/g, " ")
      .slice(0, 80);
    branchParticipants = visibleParticipants
      .filter(
        (participant) =>
          !message.target_participant_id ||
          participant.id === message.target_participant_id ||
          participant.id === message.participant_id,
      )
      .map((participant) => participant.id);
  }
  function autoGrow(node: HTMLTextAreaElement, _value: string) {
    const resize = () => {
      node.style.height = "auto";
      const maxHeight = Math.min(window.innerHeight * 0.3, 200);
      node.style.height = `${Math.min(node.scrollHeight + 2, maxHeight)}px`;
      node.style.overflowY = node.scrollHeight > maxHeight ? "auto" : "hidden";
    };
    const observer = new ResizeObserver(resize);
    if (node.parentElement) observer.observe(node.parentElement);
    window.addEventListener("resize", resize);
    node.addEventListener("input", resize);
    queueMicrotask(resize);
    return {
      update: resize,
      destroy() {
        observer.disconnect();
        window.removeEventListener("resize", resize);
        node.removeEventListener("input", resize);
      },
    };
  }
  $effect(() => {
    const roomId = selected.id;
    const count = visibleMessages.length;
    const sidechatId = activeSidechatId;
    untrack(() => {
      const shouldScroll =
        previousRoom !== roomId ||
        previousSidechat !== sidechatId ||
        !feed ||
        feed.scrollHeight - feed.scrollTop - feed.clientHeight < 80;
      previousRoom = roomId;
      previousSidechat = sidechatId;
      if (shouldScroll)
        void tick().then(() => {
          if (feed) feed.scrollTop = feed.scrollHeight;
        });
    });
    void count;
  });
</script>

<Card class="flex min-h-0 flex-1 flex-col overflow-hidden p-3"
  ><div class="mb-2 flex shrink-0 flex-wrap items-center justify-between gap-3">
    <label class="min-w-0 flex-1 text-sm">
      <span class="sr-only">{tr("room_conversation")}</span>
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
    </label>
    <span class="text-xs text-muted-foreground">{visibleMessages.length}</span>
  </div>
  <div class="mb-2 flex shrink-0 flex-wrap gap-2" aria-label={tr("room_participants")}>
    {#each visibleParticipants as participant (participant.id)}
      <span class="rounded-full border bg-muted/40 px-3 py-1 text-xs"
        >{participant.name} · {roomAgentStateLabel(participant.state)}</span
      >
    {/each}
    {#if selected.paused}<p class="w-full text-xs text-muted-foreground">
        {tr("room_chatPaused")}
      </p>{/if}
  </div>
  {#if sourceMessage}<aside class="mb-2 shrink-0 rounded-md border bg-muted/30 p-3 text-xs">
      <p class="font-medium">{tr("room_branchedFrom", { name: sourceMessage.sender })}</p>
      <p class="mt-1 line-clamp-3 whitespace-pre-wrap text-muted-foreground">
        {(
          roomBriefing(sourceMessage.body)?.objective ?? readableProtocolOutput(sourceMessage.body)
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
            ><input
              type="checkbox"
              bind:group={branchParticipants}
              value={participant.id}
            />{participant.name}</label
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
  <div class="mb-2 min-h-0 flex-1 space-y-2 overflow-y-auto overflow-x-hidden" bind:this={feed}>
    {#if visibleMessages.length === 0}<p class="text-sm text-muted-foreground">
        {tr("room_noMessages")}
      </p>{/if}{#each visibleMessages as message (message.id)}{@const briefing = roomBriefing(
        message.body,
      )}
      <article
        class="room-message {message.participant_id === null
          ? 'room-message-sent'
          : 'room-message-received'} rounded-xl border p-3"
        style={`border-left-color: ${
          message.participant_id
            ? ["#5677c8", "#b36a9a", "#538b70", "#bd8250", "#7481aa", "#ad665d"][
                Math.max(
                  0,
                  selected.participants.findIndex((p) => p.id === message.participant_id),
                ) % 6
              ]
            : "hsl(var(--primary))"
        }`}
      >
        <div class="flex flex-wrap justify-between gap-3 text-xs text-muted-foreground">
          <span
            >{message.sender}{#if message.target_participant_id}
              · {tr("room_directedMessage", {
                name:
                  selected.participants.find((p) => p.id === message.target_participant_id)?.name ??
                  message.target_participant_id,
              })}{/if}</span
          ><time>{new Date(message.created_at).toLocaleString()}</time>
        </div>
        {#if briefing}<RoomBriefing {briefing} />{:else}<div
            class="prose-chat mt-1 min-w-0 text-sm"
          >
            <MarkdownContent
              text={readableProtocolOutput(message.body)}
              basePath={selected.repo_path}
            />
          </div>{/if}
        <button
          type="button"
          class="mt-2 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          disabled={actionsDisabled}
          onclick={() => startBranch(message)}>{tr("room_branchSidechat")}</button
        >
      </article>{/each}
  </div>
  <form class="shrink-0 border-t pt-2" onsubmit={submitMessage}>
    <div class="flex min-w-0 flex-wrap items-end gap-2">
      <label class="min-w-0 w-32 flex-none space-y-1 text-xs text-muted-foreground"
        ><span class="sr-only">{tr("room_target")}</span><select
          class="min-h-9 w-full min-w-[90px] rounded-md border bg-background px-2 text-sm text-foreground"
          bind:value={targetParticipantId}
          ><option value="">{tr("room_everyone")}</option
          >{#each visibleParticipants as p (p.id)}<option value={p.id}>{p.name}</option
            >{/each}</select
        ></label
      >
      <div class="min-w-0 flex-1 basis-36">
        <label
          ><span class="sr-only">{tr("room_messagePlaceholder")}</span><textarea
            use:autoGrow={humanMessage}
            aria-label={tr("room_messagePlaceholder")}
            class="block min-h-9 max-h-[min(30dvh,200px)] w-full min-w-0 resize-none overflow-y-hidden rounded-md border bg-background px-3 py-1.5 text-sm"
            rows="1"
            wrap="soft"
            bind:value={humanMessage}
            placeholder={tr("room_messagePlaceholder")}
            disabled={actionsDisabled}
            onkeydown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
                event.preventDefault();
                if (humanMessage.trim() && !actionsDisabled)
                  event.currentTarget.form?.requestSubmit();
              }
            }}
          ></textarea></label
        >
      </div>
      <Button
        size="sm"
        class="min-h-9 min-w-[90px]"
        disabled={!humanMessage.trim() || actionsDisabled}
        loading={busyAction === "message"}>{tr("room_sendMessage")}</Button
      >
    </div>
  </form></Card
>
