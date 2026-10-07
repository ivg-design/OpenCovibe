<script lang="ts">
  import { onMount } from "svelte";
  import { renderMarkdown } from "$lib/utils/markdown";
  import forkReadme from "../../../docs/FORK.md?raw";
  import { modalFocus } from "$lib/utils/modal-focus";

  let { open = $bindable(false) }: { open: boolean } = $props();

  let appVersion = $state("");
  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      appVersion = await getVersion();
    } catch {
      appVersion = "";
    }
  });

  /** Fix image paths for Tauri webview and remove redundant language switcher. */
  function processReadme(html: string): string {
    return html
      .replace(/src="static\//g, 'src="/')
      .replace(/<p align="center">[\s\S]*?<\/p>/g, (match) =>
        match.includes("README") ? "" : match,
      )
      .replace(/href="LICENSE"/g, 'href="#"')
      .trim();
  }

  const readmeHtml = processReadme(renderMarkdown(forkReadme));

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) open = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    aria-label="About OpenCovibe"
    tabindex="-1"
    use:modalFocus
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
  >
    <div
      class="relative mx-3 flex max-h-[85vh] min-w-0 w-full max-w-3xl flex-col rounded-xl border border-border bg-background text-foreground shadow-2xl"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-border px-6 py-4">
        <div class="flex items-center gap-3">
          <span class="text-xs text-muted-foreground"
            >{appVersion ? `OpenCovibe v${appVersion}` : ""}</span
          >
          <span class="text-xs text-muted-foreground">Local fork</span>
        </div>
        <button
          class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          onclick={() => (open = false)}
          aria-label="Close"
        >
          <svg
            class="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg
          >
        </button>
      </div>

      <!-- Content -->
      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden px-6 py-4">
        <article
          class="prose prose-sm max-w-none break-words [--tw-prose-body:hsl(var(--foreground))] [--tw-prose-headings:hsl(var(--foreground))] [--tw-prose-bold:hsl(var(--foreground))] [--tw-prose-links:hsl(var(--primary))] [--tw-prose-code:hsl(var(--foreground))] [--tw-prose-bullets:hsl(var(--muted-foreground))] prose-a:underline prose-a:underline-offset-2"
        >
          {@html readmeHtml}
        </article>
      </div>

      <!-- Footer -->
      <div
        class="flex flex-wrap items-center justify-between gap-2 border-t border-border px-6 py-3 text-xs text-muted-foreground"
      >
        <span>Apache License 2.0</span>
        <span>Copyright 2025-2026 OpenCovibe Contributors</span>
      </div>
    </div>
  </div>
{/if}
