<script setup lang="ts">
import Icon, { type IconName } from "./Icon.vue";

defineProps<{ kind: "warn" | "err" | "info"; icon?: IconName; role?: "alert" | "status" | "note" }>();
</script>

<template>
  <div class="note" :data-kind="kind" :role="role">
    <Icon :name="icon ?? (kind === 'info' ? 'info' : 'warning')" :size="20" />
    <div class="note-text"><slot /></div>
  </div>
</template>
