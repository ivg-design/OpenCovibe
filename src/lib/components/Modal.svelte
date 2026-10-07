<script lang="ts">
  import { modalFocus } from "$lib/utils/modal-focus";
  let {
    open = $bindable(false),
    title = "",
    closeable = true,
    children,
  }: {
    open?: boolean;
    title?: string;
    closeable?: boolean;
    children?: import("svelte").Snippet;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (!closeable) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      open = false;
    }
  }

  function handleBackdropClick() {
    if (!closeable) return;
    open = false;
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    role="dialog"
    aria-modal="true"
    aria-label={title || "Dialog"}
    tabindex="-1"
    use:modalFocus
    onkeydown={handleKeydown}
  >
    <!-- Backdrop -->
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
      onclick={handleBackdropClick}
      role="presentation"
    ></div>

    <!-- Content -->
    <div
      class="relative z-50 max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-lg overflow-y-auto rounded-lg border bg-background p-4 shadow-lg sm:p-6"
    >
      {#if title}
        <h2 class="mb-4 text-lg font-semibold">{title}</h2>
      {/if}
      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
{/if}
