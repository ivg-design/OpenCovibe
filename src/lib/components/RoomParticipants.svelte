<script lang="ts">
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import Input from "$lib/components/Input.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { roomAgentStateLabel } from "$lib/rooms/state-label";
  import type { RoomParticipant } from "$lib/rooms/types";

  let {
    participants,
    claims,
    disabled,
    roomPaused,
    busyAction,
    onAction,
    onAdd,
  }: {
    participants: RoomParticipant[];
    claims: { task_id: string; participant_id: string; state: string }[];
    disabled: boolean;
    roomPaused: boolean;
    busyAction: string;
    onAction: (action: string, participant: RoomParticipant, message?: string) => void;
    onAdd: (input: {
      name: string;
      provider: "codex" | "claude";
      model: string | null;
      effort: string | null;
      use_worktree: boolean;
      max_turns: number;
    }) => void;
  } = $props();
  let name = $state("");
  let provider = $state<"codex" | "claude">("codex");
  let model = $state("");
  let effort = $state("");
  let useWorktree = $state(false);
  let maxTurns = $state("10");
  let wakeMessages = $state<Record<string, string>>({});

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!name.trim() || !Number.isFinite(Number(maxTurns)) || Number(maxTurns) < 1) return;
    onAdd({
      name: name.trim(),
      provider,
      model: model.trim() || null,
      effort: effort.trim() || null,
      use_worktree: useWorktree,
      max_turns: Number(maxTurns),
    });
    name = "";
  }
  const active = (p: RoomParticipant) =>
    !p.paused &&
    ["running", "working", "active", "starting", "busy"].includes(p.state.toLowerCase());
</script>

<section class="space-y-3">
  <div class="flex items-center justify-between">
    <h2 class="text-base font-semibold">{t("room_participants")}</h2>
    <span class="text-xs text-muted-foreground">{participants.length}</span>
  </div>
  <form class="room-form grid gap-2 rounded-lg border bg-card p-3" onsubmit={submit}>
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_participantName")}</span><Input bind:value={name} /></label
    >
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_provider")}</span><select
        class="h-9 rounded-md border bg-background px-2 text-sm text-foreground"
        bind:value={provider}
        ><option value="codex">Codex</option><option value="claude">Claude</option></select
      ></label
    >
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_modelOptional")}</span><Input
        bind:value={model}
        placeholder={t("room_modelPlaceholder")}
      /></label
    >
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_effortOptional")}</span><Input
        bind:value={effort}
        placeholder={provider === "codex"
          ? "minimal / low / medium / high / xhigh"
          : "low / medium / high"}
      /></label
    >
    <label class="min-w-0 space-y-1 text-xs text-muted-foreground"
      ><span>{t("room_maxTurns")}</span><Input type="number" bind:value={maxTurns} /></label
    >
    <label class="flex items-center gap-2 text-sm"
      ><input type="checkbox" bind:checked={useWorktree} />{t("room_useWorktree")}</label
    >
    <div class="sm:col-span-2 lg:col-span-3">
      <Button disabled={disabled || !name.trim()} loading={busyAction === "add-participant"}
        >{t("room_addParticipant")}</Button
      >
    </div>
  </form>
  {#if participants.length === 0}<Card variant="subtle" class="p-4 text-sm text-muted-foreground"
      >{t("room_noParticipants")}</Card
    >{/if}
  <div class="room-peer-grid grid gap-3">
    {#each participants as participant, index (participant.id)}
      {@const isActive = active(participant)}
      {@const claimsOwned = claims.filter(
        (claim) =>
          claim.participant_id === participant.id &&
          !["done", "released"].includes(claim.state.toLowerCase()),
      )}
      <Card class="space-y-3 p-4">
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0">
            <h3 class="font-medium text-foreground">
              <span
                class="mr-2 inline-block h-2.5 w-2.5 rounded-full"
                style={`background-color: ${["#5677c8", "#b36a9a", "#538b70", "#bd8250", "#7481aa", "#ad665d"][index % 6]}`}
                aria-hidden="true"
              ></span>{participant.name}
            </h3>
            <p class="text-xs text-muted-foreground">
              {participant.provider} · {participant.model ?? t("room_defaultModel")} · {participant.effort ??
                t("room_defaultEffort")}
            </p>
          </div>
          <span class="rounded bg-muted px-2 py-1 text-xs"
            >{roomAgentStateLabel(participant.state)}</span
          >
        </div>
        <div class="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div>
            {t("room_budget", {
              used: String(participant.wake_count),
              max: String(participant.max_turns),
            })}
          </div>
          <div>
            {participant.branch
              ? t("room_branch", { branch: participant.branch })
              : t("room_noBranch")}
          </div>
        </div>
        {#if participant.last_error}<p
            class="rounded border border-destructive/30 bg-destructive/5 p-2 text-xs text-destructive"
            role="status"
          >
            {participant.last_error}
          </p>{/if}
        {#if participant.pending_delivery}<p
            class="rounded border border-amber-500/30 bg-amber-500/5 p-2 text-xs"
          >
            {t("room_pendingDelivery", { reason: participant.pending_delivery.reason })}
          </p>{/if}
        <div class="flex flex-wrap gap-2">
          <a
            class="inline-flex h-8 items-center rounded-md border px-3 text-xs hover:bg-accent"
            href={`/chat?run=${encodeURIComponent(participant.run_id)}`}>{t("room_openSession")}</a
          >
          <Button
            size="sm"
            variant="outline"
            disabled={disabled ||
              isActive ||
              !participant.worktree_path ||
              (!roomPaused && !participant.paused)}
            onclick={() => onAction("merge", participant)}>{t("room_mergeWorktree")}</Button
          >
          <Button
            size="sm"
            variant="outline"
            {disabled}
            onclick={() =>
              onAction(
                participant.paused ? "resume-participant" : "pause-participant",
                participant,
              )}
            >{participant.paused ? t("room_resumeParticipant") : t("room_pauseParticipant")}</Button
          >
          <Button
            size="sm"
            variant="outline"
            disabled={disabled || isActive || claimsOwned.length > 0}
            onclick={() => onAction("remove-participant", participant)}
            >{t("room_removeParticipant")}</Button
          >
        </div>
        <form
          class="flex flex-wrap gap-2"
          onsubmit={(event) => {
            event.preventDefault();
            const message = wakeMessages[participant.id]?.trim();
            if (message) {
              onAction("wake", participant, message);
              wakeMessages[participant.id] = "";
            }
          }}
        >
          <label class="min-w-0 flex-1"
            ><span class="sr-only">{t("room_wakeMessageLabel", { name: participant.name })}</span
            ><Input
              class="w-full"
              value={wakeMessages[participant.id] ?? ""}
              oninput={(event) => {
                wakeMessages[participant.id] = (event.currentTarget as HTMLInputElement).value;
              }}
              placeholder={t("room_wakeMessagePlaceholder")}
            /></label
          >
          <Button size="sm" disabled={disabled || !wakeMessages[participant.id]?.trim()}
            >{t("room_wake")}</Button
          >
        </form>
        {#if claimsOwned.length > 0}<div class="space-y-1 border-t pt-2">
            <p class="text-xs font-medium">{t("room_ownedClaims")}</p>
            {#each claimsOwned as claim (claim.task_id)}<div
                class="flex items-center justify-between gap-2 text-xs"
              >
                <span>{claim.task_id} · {claim.state}</span><Button
                  size="sm"
                  variant="outline"
                  disabled={disabled || isActive || (!roomPaused && !participant.paused)}
                  onclick={() => onAction(`release:${claim.task_id}`, participant)}
                  >{t("room_releaseClaim")}</Button
                >
              </div>{/each}
          </div>{/if}
      </Card>
    {/each}
  </div>
</section>
