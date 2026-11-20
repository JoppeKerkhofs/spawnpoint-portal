<script setup lang="ts">
import { codeGroups, codeSpoken } from "../logic/code";

defineProps<{ code: string }>();
</script>

<template>
  <span>
    <span class="code" aria-hidden="true">{{ codeGroups(code) }}</span>
    <span class="sr-only">{{ codeSpoken(code) }}</span>
  </span>
</template>
