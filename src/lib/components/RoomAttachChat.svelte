<script lang="ts">
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import { listAttachableRoomChats } from "$lib/rooms/api";
  import type { AttachableRoomChat } from "$lib/rooms/types";
  import { t } from "$lib/i18n/index.svelte";

  let {
    roomId,
    disabled,
    onAttach,
  }: {
    roomId: string;
    disabled: boolean;
    onAttach: (runId: string, name: string) => Promise<boolean>;
  } = $props();
  let open = $state(false),
    loading = $state(false),
    error = $state("");
  let chats = $state<AttachableRoomChat[]>([]),
    selectedId = $state(""),
    name = $state(""),
    query = $state("");
  let generation = 0;
  let previousRoomId = "";
  const matches = $derived(
    chats.filter((c) =>
      `${c.name} ${c.title} ${c.provider} ${c.cwd}`
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
    ),
  );
  $effect(() => {
    if (!roomId || roomId === previousRoomId) return;
    previousRoomId = roomId;
    generation++;
    open = false;
    chats = [];
    selectedId = name = query = error = "";
    loading = false;
  });
  async function load() {
    const id = roomId,
      request = ++generation;
    open = loading = true;
    error = "";
    selectedId = name = "";
    try {
      const result = await listAttachableRoomChats(id);
      if (request === generation && id === roomId) chats = result;
    } catch (cause) {
      if (request === generation) error = String(cause);
    } finally {
      if (request === generation) loading = false;
    }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedId || !name.trim() || disabled || loading) return;
    if (await onAttach(selectedId, name.trim())) {
      open = false;
      selectedId = name = "";
    }
  }
</script>

<section class="min-w-0 space-y-2 rounded-lg border bg-card p-2.5">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="min-w-0">
      <h2 class="text-sm font-semibold">{t("room_attachExisting")}</h2>
      <p class="text-xs text-muted-foreground">{t("room_attachExistingHelp")}</p>
    </div>
    <Button
      size="sm"
      variant="outline"
      {disabled}
      onclick={() => (open ? (open = false) : void load())}
      >{open ? t("common_cancel") : t("room_chooseExisting")}</Button
    >
  </div>
  {#if open}
    <form class="min-w-0 space-y-2" onsubmit={submit}>
      <label class="block space-y-1 text-xs text-muted-foreground">
        <span>{t("room_searchChats")}</span><Input
          bind:value={query}
          disabled={loading || disabled}
        />
      </label>
      {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
      {#if loading}<p class="text-xs text-muted-foreground" role="status">{t("room_loading")}</p>
      {:else if !matches.length}<p class="text-xs text-muted-foreground">
          {t("room_noAttachableChats")}
        </p>
      {:else}
        <div
          class="max-h-64 space-y-1 overflow-y-auto overflow-x-hidden"
          aria-label={t("room_chooseExisting")}
        >
          {#each matches as chat (chat.run_id)}
            <button
              type="button"
              class="chat-choice w-full min-w-0 rounded-md border px-2 py-1.5 text-left"
              class:chosen={selectedId === chat.run_id}
              aria-pressed={selectedId === chat.run_id}
              {disabled}
              onclick={() => {
                selectedId = chat.run_id;
                name = chat.name.slice(0, 80);
              }}
            >
              <span class="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-sm">
                <strong class="min-w-0 break-words">{chat.name}</strong>
                <span class="text-xs text-muted-foreground"
                  >{chat.provider}{chat.model ? ` · ${chat.model}` : ""}</span
                >
                {#if chat.previous_participant}<span class="text-xs text-primary"
                    >{t("room_previousParticipant")}</span
                  >{/if}
              </span>
              {#if chat.title !== chat.name}<span
                  class="mt-1 block text-xs text-muted-foreground break-words">{chat.title}</span
                >{/if}
              <span class="mt-1 block truncate text-xs text-muted-foreground" title={chat.cwd}
                >{chat.cwd}</span
              >
            </button>
          {/each}
        </div>
      {/if}
      {#if selectedId}
        <label class="block space-y-1 text-xs text-muted-foreground"
          ><span>{t("room_participantName")}</span><Input bind:value={name} {disabled} /></label
        >
        <Button
          size="sm"
          disabled={disabled || !name.trim() || name.trim().length > 80}
          type="submit">{t("room_attachPaused")}</Button
        >
      {/if}
    </form>
  {/if}
</section>

<style>
  .chat-choice.chosen {
    border-color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.1);
  }
</style>
