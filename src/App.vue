<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, watch } from "vue";
import { onGameExited, settingsGet } from "./api";
import Sidebar from "./components/Sidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import TitleBar from "./components/TitleBar.vue";
import ViewHead from "./components/ViewHead.vue";
import { gameExited, loadPlaying, openInstanceId, playerName, view } from "./logic/operation";

// Moves focus to the new view's h1 so a screen reader hears where it is.
watch([view, openInstanceId], () => nextTick(() => document.querySelector<HTMLElement>(".main h1")?.focus()));

let unlisten: (() => void) | undefined;
onMounted(async () => {
  unlisten = await onGameExited((e) => gameExited(e.instance));
  loadPlaying();
  try {
    playerName.value = (await settingsGet()).player_name;
  } catch {
    // The footer stays empty; the Account view reports a failing settings call itself.
  }
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <div class="win">
    <TitleBar />
    <div class="shell-body">
      <Sidebar />
      <main class="main">
        <!-- Placeholders: Tasks 14 to 18 replace each branch with its view. -->
        <ViewHead v-if="view === 'servers'" title="Servers" summary="" />
        <ViewHead v-else-if="view === 'instances'" title="Instances" summary="" />
        <ViewHead v-else-if="view === 'instance'" title="Instance" summary="" />
        <ViewHead v-else-if="view === 'panels'" title="Panels" summary="" />
        <ViewHead v-else title="Account" summary="" />
        <div class="content"></div>
      </main>
    </div>
    <StatusBar />
  </div>
</template>
