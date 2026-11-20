<script setup lang="ts">
import { computed } from "vue";
import { initials } from "../logic/visuals";
import { playerName, view, type View } from "../logic/operation";
import Icon, { type IconName } from "./Icon.vue";
import Logo from "./Logo.vue";

const items: { id: View; label: string; icon: IconName }[] = [
  { id: "servers", label: "Servers", icon: "server" },
  { id: "instances", label: "Instances", icon: "box" },
  { id: "panels", label: "Panels", icon: "link" },
  { id: "account", label: "Account", icon: "user" },
];

const active = computed(() => (view.value === "instance" ? "instances" : view.value));
</script>

<template>
  <nav class="side" aria-label="Main">
    <div class="brand"><Logo :size="26" /><span>Spawnpoint Portal</span></div>
    <button
      v-for="i in items"
      :key="i.id"
      type="button"
      class="nav"
      :aria-label="i.label"
      :title="i.label"
      :aria-current="active === i.id ? 'page' : undefined"
      @click="view = i.id"
    >
      <Icon :name="i.icon" :size="18" /><span class="nav-label">{{ i.label }}</span>
    </button>
    <button
      type="button"
      class="side-foot"
      :aria-label="`Account, ${playerName}`"
      :title="playerName"
      :aria-current="view === 'account' ? 'page' : undefined"
      @click="view = 'account'"
    >
      <span class="avatar" aria-hidden="true">{{ initials(playerName) }}</span>
      <span class="side-text" style="min-width: 0">
        <span class="side-name mc-name">{{ playerName }}</span>
        <span class="side-sub">Offline name</span>
      </span>
    </button>
  </nav>
</template>
