<script setup lang="ts">
defineProps<{ title: string; summary: string }>();
</script>

<template>
  <div class="head" :class="{ 'is-centred': $slots.lead }">
    <div v-if="$slots.back" class="head-back"><slot name="back" /></div>
    <div v-if="$slots.lead" class="head-lead"><slot name="lead" /></div>
    <div class="head-text">
      <h1 tabindex="-1" :title="title" class="truncate">{{ title }}</h1>
      <p v-if="summary" class="head-summary">{{ summary }}</p>
    </div>
    <div class="head-actions"><slot name="actions" /></div>
  </div>
</template>
