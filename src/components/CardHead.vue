<script setup lang="ts">
defineProps<{ title: string; meta?: string; headingId?: string }>();
</script>

<template>
  <div class="card-head">
    <h2 :id="headingId" class="card-title mc-name truncate" :title="title">{{ title }}</h2>
    <span v-if="meta" class="card-meta tabular">{{ meta }}</span>
    <span class="flex-1" />
    <slot name="actions" />
  </div>
</template>
