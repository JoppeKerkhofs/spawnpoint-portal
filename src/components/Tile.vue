<script setup lang="ts">
import { computed } from "vue";
import type { Texture } from "../logic/visuals";

const props = withDefaults(defineProps<{ name: string; texture: Texture; error?: boolean; size?: 24 | 32 | 44 }>(), {
  error: false,
  size: 32,
});

const initial = computed(() => Array.from(props.name)[0] ?? "");
const tone = computed(() => (props.error ? "error" : props.texture));
</script>

<template>
  <span
    class="tile"
    :class="{ 'tile-44': size === 44 }"
    :style="{ width: `${size}px`, height: `${size}px`, '--tile-bg': `var(--tile-${tone}-bg)`, '--tile-fg': `var(--tile-${tone}-fg)` }"
    aria-hidden="true"
    >{{ initial }}</span
  >
</template>
