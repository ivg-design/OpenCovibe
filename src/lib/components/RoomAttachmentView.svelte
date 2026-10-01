<script lang="ts">
  import { onMount } from "svelte";
  import Button from "$lib/components/Button.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { formatBytes } from "$lib/utils/format";
  import { openRoomAttachment, readRoomAttachment } from "$lib/rooms/api";
  import type { RoomAttachment } from "$lib/rooms/types";

  let {
    roomId,
    attachment,
    onremove,
  }: { roomId: string; attachment: RoomAttachment; onremove?: () => void } = $props();
  const imageTypes = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);
  let visible = $state(false);
  let previewUrl = $state("");
  let failed = $state(false);
  let opening = $state(false);
  let request = 0;
  let attachmentElement: HTMLDivElement;
  const isPreviewable = $derived(imageTypes.has(attachment.mime_type.toLowerCase()));

  onMount(() => {
    if (!isPreviewable) return;
    ++request;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) visible = true;
      },
      { rootMargin: "160px" },
    );
    observer.observe(attachmentElement);
    return () => {
      request++;
      observer.disconnect();
      if (previewUrl) URL.revokeObjectURL(previewUrl);
    };
  });

  $effect(() => {
    if (!visible || !isPreviewable || previewUrl || failed) return;
    const currentRequest = ++request;
    const currentRoom = roomId;
    const currentId = attachment.id;
    void readRoomAttachment(currentRoom, currentId)
      .then((data) => {
        if (currentRequest !== request) return;
        const binary = atob(data.contentBase64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
        previewUrl = URL.createObjectURL(new Blob([bytes], { type: data.type }));
      })
      .catch(() => {
        if (currentRequest === request) failed = true;
      });
  });

  async function showFile() {
    opening = true;
    failed = false;
    try {
      await openRoomAttachment(roomId, attachment.id);
    } catch {
      failed = true;
    } finally {
      opening = false;
    }
  }
</script>

<div
  bind:this={attachmentElement}
  class="flex min-w-0 max-w-full flex-wrap items-center gap-2 rounded-md border bg-muted/30 px-2 py-1.5 text-xs"
>
  {#if isPreviewable}
    {#if previewUrl}<img
        src={previewUrl}
        alt={attachment.name}
        loading="lazy"
        class="max-h-40 max-w-full rounded object-contain"
      />
    {:else}<span class="text-muted-foreground"
        >{failed ? t("room_attachmentPreviewError") : t("room_imageAttachment")}</span
      >{/if}
  {/if}
  <span
    class="min-w-0 flex-1 break-words font-medium [overflow-wrap:anywhere]"
    title={attachment.name}>{attachment.name}</span
  >
  <span class="shrink-0 text-muted-foreground">{formatBytes(attachment.size)}</span>
  {#if onremove}<Button
      size="sm"
      variant="ghost"
      disabled={opening}
      onclick={onremove}
      class="shrink-0 whitespace-nowrap"
      ><span class="sr-only">{t("common_removeAttachment")}</span>×</Button
    >{/if}
  <Button
    size="sm"
    variant="ghost"
    class="shrink-0 whitespace-nowrap"
    disabled={opening}
    onclick={() => void showFile()}
  >
    {#if opening}<svg
        class="mr-1 h-3.5 w-3.5 animate-spin"
        viewBox="0 0 24 24"
        fill="none"
        aria-hidden="true"
        ><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"
        ></circle><path
          class="opacity-75"
          fill="currentColor"
          d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
        ></path></svg
      >{/if}
    {t("room_showAttachment")}
  </Button>
</div>
