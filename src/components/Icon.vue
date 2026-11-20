<script lang="ts">
export type IconName =
  | "refresh" | "plus" | "play" | "folder" | "trash" | "upload" | "lock" | "external" | "check"
  | "warning" | "info" | "chevron-left" | "server" | "box" | "link" | "user" | "minimize" | "maximize" | "close";
</script>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(defineProps<{ name: IconName; size?: number }>(), { size: 16 });

const paths: Record<IconName, string[]> = {
  refresh: ["M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5"],
  plus: ["M12 5v14M5 12h14"],
  play: ["M7 5v14l12-7z"],
  folder: ["M3 6.5A1.5 1.5 0 0 1 4.5 5H10l2 2h7.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"],
  trash: ["M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"],
  upload: ["M12 16V4M7 9l5-5 5 5M4 20h16"],
  lock: ["M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z", "M8 11V7a4 4 0 0 1 8 0v4"],
  external: ["M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"],
  check: ["M4 12l5 5L20 6"],
  warning: ["M12 3 2 20h20z", "M12 10v4M12 17.5v.5"],
  info: ["M3 12a9 9 0 1 0 18 0 9 9 0 1 0-18 0", "M12 11v6M12 7.5v.5"],
  "chevron-left": ["M15 5l-7 7 7 7"],
  server: ["M4 5h16v5H4zM4 14h16v5H4z", "M8 7.5h.01M8 16.5h.01"],
  box: ["M12 3 4 7.5v9L12 21l8-4.5v-9z", "M4 7.5 12 12l8-4.5M12 12v9"],
  link: ["M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1", "M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"],
  user: ["M8 8a4 4 0 1 0 8 0 4 4 0 1 0-8 0", "M4 21c1-4 4.5-6 8-6s7 2 8 6"],
  minimize: ["M0 5h10"],
  maximize: ["M.5 .5h9v9h-9z"],
  close: ["M0 0l10 10M10 0 0 10"],
};

// The title bar's three marks sit on a 10 box with a 1px stroke; the rest on a 24 box with stroke 2.
const windowMark = computed(() => ["minimize", "maximize", "close"].includes(props.name));
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    :viewBox="windowMark ? '0 0 10 10' : '0 0 24 24'"
    fill="none"
    stroke="currentColor"
    :stroke-width="windowMark ? 1 : 2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path v-for="d in paths[name]" :key="d" :d="d" />
  </svg>
</template>
