<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, watch } from "vue";
import { onGameExited, settingsGet } from "./api";
import Sidebar from "./components/Sidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import TitleBar from "./components/TitleBar.vue";
import InstanceView from "./views/InstanceView.vue";
import InstancesView from "./views/InstancesView.vue";
import AccountView from "./views/AccountView.vue";
import PanelsView from "./views/PanelsView.vue";
import ServersView from "./views/ServersView.vue";
import { gameExited, loadPlaying, openInstanceId, playerName, view } from "./logic/operation";

// Moves focus to the new view's h1 so a screen reader hears where it is.
watch([view, openInstanceId], () => nextTick(() => document.querySelector<HTMLElement>(".main h1")?.focus()));

let unlisten: (() => void) | undefined;
onMounted(async () => {
  loadPlaying();
  settingsGet().then(
    (s) => (playerName.value = s.player_name),
    () => {}, // The footer stays empty; the Account view reports a failing settings call itself.
  );
  try {
    unlisten = await onGameExited((e) => gameExited(e.instance));
  } catch {
    // A game that exits then stays "Playing" until the window reloads.
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
        <ServersView v-if="view === 'servers'" />
        <InstancesView v-else-if="view === 'instances'" />
        <InstanceView v-else-if="view === 'instance'" :key="openInstanceId ?? undefined" />
        <PanelsView v-else-if="view === 'panels'" />
        <AccountView v-else />
      </main>
    </div>
    <StatusBar />
  </div>
</template>
