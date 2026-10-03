<script lang="ts">
  import type { RoomSidebarEntry } from "$lib/rooms/types";
  import { roomParticipantColorAt } from "$lib/utils/room-participant-colors";
  import { roomAgentStateLabel } from "$lib/rooms/state-label";
  import SidebarAgentStatus from "./SidebarAgentStatus.svelte";
  import { sidebarRoomState } from "$lib/utils/sidebar-status";
  import { t } from "$lib/i18n/index.svelte";
  let {
    room,
    selectedRoomId,
    selectedRunId,
    onSelectRoom,
    onSelectParticipant,
  }: {
    room: RoomSidebarEntry;
    selectedRoomId: string;
    selectedRunId: string;
    onSelectRoom: () => void;
    onSelectParticipant: (runId: string) => void;
  } = $props();
  let expanded = $state(true);
  const activeSelection = $derived(
    room.id === selectedRoomId
      ? `room:${selectedRoomId}`
      : room.participants.some((p) => p.run_id === selectedRunId)
        ? `run:${selectedRunId}`
        : "",
  );
  $effect(() => {
    // Track selection, not the periodically refreshed room object.
    if (activeSelection) expanded = true;
  });
</script>

<div class="my-1 min-w-0">
  <div
    class="flex min-w-0 items-center rounded-md {selectedRoomId === room.id
      ? 'bg-sidebar-accent text-sidebar-accent-foreground'
      : 'hover:bg-sidebar-accent/50'}"
  >
    <button
      type="button"
      class="shrink-0 rounded p-1 text-muted-foreground"
      aria-expanded={expanded}
      aria-label={t("room_sidebarParticipants", { name: room.title })}
      onclick={() => {
        expanded = !expanded;
      }}
    >
      {expanded ? "▾" : "▸"}
    </button>
    <button
      type="button"
      class="flex min-w-0 flex-1 items-center gap-1.5 py-2 pr-2 text-left text-xs"
      title={room.title}
      aria-current={selectedRoomId === room.id ? "page" : undefined}
      onclick={onSelectRoom}
    >
      <svg
        class="h-3.5 w-3.5 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.7"
        aria-hidden="true"
        ><circle cx="8" cy="7" r="3" /><path
          d="M2 21v-3a6 6 0 0 1 12 0v3M17 4a3 3 0 0 1 0 6M17 14a5 5 0 0 1 5 5v2"
        /></svg
      >
      <span class="min-w-0 flex-1 truncate">{room.title}</span>
      <span class="sr-only">{t("room_sidebarRoom")}</span>
      <SidebarAgentStatus state={sidebarRoomState(room)} />
      {#if room.needs_answer}<span
          class="shrink-0 rounded bg-amber-500/20 px-1 text-amber-600 dark:text-amber-400"
          title={t("room_requestsNeedYou", { count: String(room.needs_answer) })}
          >{room.needs_answer}</span
        >{/if}
    </button>
  </div>
  {#if expanded}<div class="ml-3 border-l pl-1">
      {#each room.participants as participant (participant.participant_id)}
        <button
          type="button"
          class="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs {selectedRunId ===
          participant.run_id
            ? 'bg-sidebar-accent text-sidebar-accent-foreground'
            : 'text-sidebar-foreground hover:bg-sidebar-accent/50'}"
          title={`${participant.name} · ${participant.provider} · ${roomAgentStateLabel(participant.state)}`}
          onclick={() => onSelectParticipant(participant.run_id)}
        >
          <span
            class="h-2 w-2 shrink-0 rounded-full"
            style={`background: ${roomParticipantColorAt(participant.color_index, participant.participant_id)}`}
          ></span>
          <span class="min-w-0 flex-1 truncate">{participant.name}</span>
          <span class="shrink-0 text-[10px] text-muted-foreground">{participant.provider}</span>
          <SidebarAgentStatus state={participant.state} />
        </button>
      {/each}
      {#if !room.participants.length}<p class="px-2 py-1 text-xs text-muted-foreground">
          {t("room_noParticipants")}
        </p>{/if}
    </div>{/if}
</div>
