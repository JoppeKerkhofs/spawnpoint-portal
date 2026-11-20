<script setup lang="ts">
import type { PillTone } from "../logic/visuals";

defineProps<{ tone: PillTone }>();
</script>

<template>
  <span class="pill" :data-tone="tone">
    <svg v-if="tone === 'error'" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true">
      <path d="M12 6v8M12 18v.5" />
    </svg>
    <span v-else class="pill-mark" :class="tone === 'ok' ? 'is-ok' : tone === 'busy' ? 'is-busy' : ''" aria-hidden="true" />
    <slot />
  </span>
</template>
