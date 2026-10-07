<script lang="ts">
  import type { RunStatus } from "$lib/types";

  type DisplayStatus = Exclude<RunStatus, "idle"> | "waiting" | "done";

  let {
    status,
    attention = false,
    label,
    compact = false,
    class: className = "",
  }: {
    status: RunStatus;
    attention?: boolean;
    label?: string;
    compact?: boolean;
    class?: string;
  } = $props();

  const displayStatus: DisplayStatus = $derived(
    (status === "running" || status === "idle") && attention
      ? "waiting"
      : status === "idle"
        ? "done"
        : status,
  );

  const colors: Record<DisplayStatus, string> = {
    pending: "bg-amber-500/20 text-amber-600 dark:text-amber-400",
    running: "bg-blue-500/20 text-blue-600 dark:text-blue-400",
    done: "bg-cyan-500/20 text-cyan-600 dark:text-cyan-400",
    waiting: "bg-amber-500/20 text-amber-600 dark:text-amber-400",
    completed: "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400",
    failed: "bg-red-500/20 text-red-600 dark:text-red-400",
    stopped: "bg-gray-500/20 text-gray-600 dark:text-gray-400",
  };

  const dots: Record<DisplayStatus, string> = {
    pending: "bg-amber-500",
    running: "bg-blue-500 animate-pulse",
    done: "bg-cyan-500",
    waiting: "bg-amber-500 animate-pulse",
    completed: "bg-emerald-500",
    failed: "bg-red-500",
    stopped: "bg-gray-500",
  };
</script>

<span
  class="inline-flex items-center whitespace-nowrap rounded-full py-0.5 font-medium {compact
    ? 'gap-1 px-1.5 text-[10px]'
    : 'gap-1.5 px-2 text-xs'} {colors[displayStatus]} {className}"
>
  <span class="h-1.5 w-1.5 rounded-full {dots[displayStatus]}"></span>
  {label ?? displayStatus}
</span>
