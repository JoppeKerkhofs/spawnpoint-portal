<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import Icon from "./Icon.vue";
import Logo from "./Logo.vue";
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <span class="flex" data-tauri-drag-region><Logo :size="16" data-tauri-drag-region /></span>
    <span class="titlebar-title" data-tauri-drag-region>Spawnpoint Portal</span>
    <button type="button" class="titlebar-btn" aria-label="Minimize" @click="getCurrentWindow().minimize()"><Icon name="minimize" :size="10" /></button>
    <button type="button" class="titlebar-btn" aria-label="Maximize" @click="getCurrentWindow().toggleMaximize()"><Icon name="maximize" :size="10" /></button>
    <button type="button" class="titlebar-btn is-close" aria-label="Close" @click="getCurrentWindow().close()"><Icon name="close" :size="10" /></button>
  </header>
</template>
