<script setup lang="ts">
defineProps<{ kind: "own" | "pack" | "hand" }>();
</script>

<template>
  <span class="badge" :data-kind="kind"><slot /></span>
</template>
