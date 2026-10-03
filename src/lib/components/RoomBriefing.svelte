<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { RoomBriefing } from "$lib/utils/room-presentation";
  let { briefing }: { briefing: RoomBriefing } = $props();
  const reason = $derived(
    briefing.reason === "message"
      ? t("room_briefingMessage")
      : briefing.reason === "timer"
        ? t("room_briefingTimer")
        : briefing.reason === "task"
          ? t("room_briefingTask")
          : briefing.reason === "bootstrap"
            ? t("room_briefingBootstrap")
            : t("room_briefingCheckIn"),
  );
</script>

<div class="space-y-2 rounded-md border border-border bg-muted/30 p-3 text-sm">
  <p class="font-medium">
    {t("room_briefing")}{#if briefing.participant}
      · {briefing.participant}{/if}
  </p>
  <p class="text-muted-foreground">{reason}</p>
  <p class="font-medium text-xs text-muted-foreground">{t("room_briefingObjective")}</p>
  <p class="whitespace-pre-wrap break-words">{briefing.objective}</p>
</div>
