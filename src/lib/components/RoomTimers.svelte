<script lang="ts">
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { RoomParticipant, RoomTimer, SaveTimerInput } from "$lib/rooms/types";

  let {
    timers,
    participants,
    disabled,
    busyAction,
    onSave,
    onDelete,
  }: {
    timers: RoomTimer[];
    participants: RoomParticipant[];
    disabled: boolean;
    busyAction: string;
    onSave: (input: SaveTimerInput) => void;
    onDelete: (timer: RoomTimer) => void;
  } = $props();
  let editing = $state<string | null>(null);
  let participantId = $state("");
  let message = $state("");
  let interval = $state("300");
  let maxDeliveries = $state("10");
  let idleOnly = $state(true);
  let enabled = $state(true);

  function reset() {
    editing = null;
    message = "";
    interval = "300";
    maxDeliveries = "10";
    idleOnly = true;
    enabled = true;
  }
  function edit(timer: RoomTimer) {
    editing = timer.id;
    participantId = timer.participant_id;
    message = timer.message;
    interval = String(timer.interval_seconds);
    maxDeliveries = String(timer.max_deliveries);
    idleOnly = timer.idle_only;
    enabled = timer.enabled;
  }
  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (
      !participantId ||
      !message.trim() ||
      !Number.isFinite(Number(interval)) ||
      Number(interval) < 30 ||
      !Number.isFinite(Number(maxDeliveries)) ||
      Number(maxDeliveries) < 1
    )
      return;
    onSave({
      id: editing,
      participant_id: participantId,
      message: message.trim(),
      interval_seconds: Number(interval),
      idle_only: idleOnly,
      enabled,
      max_deliveries: Number(maxDeliveries),
    });
    reset();
  }
</script>

<section class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-base font-semibold">{t("room_timers")}</h2>
  </div>
  {#if participants.length === 0}<Card variant="subtle" class="p-4 text-sm text-muted-foreground"
      >{t("room_timersNeedParticipant")}</Card
    >{:else}
    <form class="grid gap-2 rounded-lg border bg-card p-3 md:grid-cols-2" onsubmit={submit}>
      <label class="space-y-1 text-xs text-muted-foreground"
        ><span>{t("room_timerParticipant")}</span><select
          class="h-9 w-full rounded-md border bg-background px-2 text-sm text-foreground"
          bind:value={participantId}
          ><option value="">{t("room_chooseParticipant")}</option
          >{#each participants as p (p.id)}<option value={p.id}>{p.name}</option>{/each}</select
        ></label
      >
      <label class="space-y-1 text-xs text-muted-foreground"
        ><span>{t("room_timerMessage")}</span><Input bind:value={message} /></label
      >
      <label class="space-y-1 text-xs text-muted-foreground"
        ><span>{t("room_intervalSeconds")}</span><Input
          type="number"
          bind:value={interval}
        /></label
      >
      <label class="space-y-1 text-xs text-muted-foreground"
        ><span>{t("room_maxDeliveries")}</span><Input
          type="number"
          bind:value={maxDeliveries}
        /></label
      >
      <div class="flex flex-wrap gap-4 md:col-span-2">
        <label class="flex items-center gap-2 text-sm"
          ><input type="checkbox" bind:checked={idleOnly} />{t("room_idleOnly")}</label
        ><label class="flex items-center gap-2 text-sm"
          ><input type="checkbox" bind:checked={enabled} />{t("room_timerEnabled")}</label
        >
      </div>
      <div class="flex gap-2 md:col-span-2">
        <Button
          disabled={disabled ||
            !participantId ||
            !Number.isFinite(Number(interval)) ||
            Number(interval) < 30 ||
            !Number.isFinite(Number(maxDeliveries)) ||
            Number(maxDeliveries) < 1}
          loading={busyAction === "save-timer"}
          >{editing ? t("common_save") : t("room_addTimer")}</Button
        >{#if editing}<button
            type="button"
            class="h-9 rounded-md border px-4 text-sm hover:bg-accent"
            onclick={reset}>{t("common_cancel")}</button
          >{/if}
      </div>
    </form>
    {#if timers.length === 0}<Card variant="subtle" class="p-4 text-sm text-muted-foreground"
        >{t("room_noTimers")}</Card
      >{/if}
    <div class="grid gap-2 md:grid-cols-2">
      {#each timers as timer (timer.id)}<Card class="space-y-2 p-3"
          ><div class="flex items-start justify-between gap-2">
            <div>
              <p class="text-sm font-medium">
                {participants.find((p) => p.id === timer.participant_id)?.name ??
                  timer.participant_id}
              </p>
              <p class="whitespace-pre-wrap text-sm">{timer.message}</p>
            </div>
            <span class="rounded bg-muted px-2 py-0.5 text-xs"
              >{timer.enabled ? t("room_timerEnabled") : t("room_timerDisabled")}</span
            >
          </div>
          <p class="text-xs text-muted-foreground">
            {t("room_timerStats", {
              interval: String(timer.interval_seconds),
              count: String(timer.delivered_count),
              max: String(timer.max_deliveries),
              mode: timer.idle_only ? t("room_idleOnly") : t("room_anyState"),
            })}
          </p>
          {#if timer.last_error}<p class="text-xs text-destructive" role="status">
              {timer.last_error}
            </p>{/if}
          <div class="flex gap-2">
            <Button size="sm" variant="outline" {disabled} onclick={() => edit(timer)}
              >{t("common_edit")}</Button
            ><Button
              size="sm"
              variant="outline"
              {disabled}
              onclick={() =>
                onSave({
                  id: timer.id,
                  participant_id: timer.participant_id,
                  message: timer.message,
                  interval_seconds: timer.interval_seconds,
                  idle_only: timer.idle_only,
                  enabled: !timer.enabled,
                  max_deliveries: timer.max_deliveries,
                })}>{timer.enabled ? t("room_disableTimer") : t("room_enableTimer")}</Button
            ><Button size="sm" variant="outline" {disabled} onclick={() => onDelete(timer)}
              >{t("room_deleteTimer")}</Button
            >
          </div></Card
        >{/each}
    </div>
  {/if}
</section>
