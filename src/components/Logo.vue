<script setup lang="ts">
import logo from "../assets/logo.svg";

defineProps<{ size: 16 | 26 | 30 | 56 }>();
</script>

<template>
  <img :src="logo" :width="size" :height="size" alt="" />
</template>
