<script setup lang="ts">
defineProps<{ rows: Array<Array<[width: string, height: string]>> }>();
</script>

<template>
  <div role="status">
    <span class="sr-only">Loading…</span>
    <div v-for="(row, r) in rows" :key="r" class="flex items-center gap-3 py-2" aria-hidden="true">
      <span
        v-for="([width, height], b) in row"
        :key="b"
        class="block shrink-0 animate-pulse rounded-xs bg-(--skeleton) motion-reduce:animate-none"
        :style="width === 'flex' ? { flex: '1 1 auto', height } : { width, height }"
      />
    </div>
  </div>
</template>
