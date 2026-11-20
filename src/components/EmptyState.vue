<script setup lang="ts">
import Logo from "./Logo.vue";

withDefaults(defineProps<{ title: string; maxWidth?: 380 | 420 }>(), { maxWidth: 380 });
</script>

<template>
  <div class="card empty">
    <Logo :size="56" />
    <h2 class="empty-title mc-name">{{ title }}</h2>
    <p class="empty-text" :style="{ maxWidth: `${maxWidth}px` }"><slot /></p>
    <slot name="action" />
  </div>
</template>
