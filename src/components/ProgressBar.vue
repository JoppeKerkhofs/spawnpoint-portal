<script setup lang="ts">
defineProps<{ value: number; max: number; label: string }>();
</script>

<template>
  <span class="progress"><progress :value="value" :max="max" :aria-label="label" /></span>
</template>
