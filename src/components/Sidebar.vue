<script setup lang="ts">
import { computed } from "vue";
import { headUrl } from "../logic/picture";
import { initials } from "../logic/visuals";
import { playerName, view, type View } from "../logic/operation";
import { availableVersion, update } from "../logic/update";
import Icon, { type IconName } from "./Icon.vue";
import Logo from "./Logo.vue";
import Picture from "./Picture.vue";

const items: { id: View; label: string; icon: IconName }[] = [
  { id: "servers", label: "Servers", icon: "server" },
  { id: "instances", label: "Instances", icon: "box" },
  { id: "panels", label: "Panels", icon: "link" },
  { id: "account", label: "Account", icon: "user" },
];

const updateReady = computed(() => availableVersion(update.value) !== null);
const active = computed(() => (view.value === "instance" ? "instances" : view.value === "server" ? "servers" : view.value));
</script>

<template>
  <nav class="side" aria-label="Main">
    <div class="brand"><Logo :size="26" /><span>Spawnpoint Portal</span></div>
    <button
      v-for="i in items"
      :key="i.id"
      type="button"
      class="nav"
      :aria-label="i.id === 'account' && updateReady ? 'Account, Update available' : i.label"
      :title="i.label"
      :aria-current="active === i.id ? 'page' : undefined"
      @click="view = i.id"
    >
      <Icon :name="i.icon" :size="18" /><span class="nav-label">{{ i.label }}</span>
      <span v-if="i.id === 'account' && updateReady" class="update-dot" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="side-foot"
      :aria-label="`Account, ${playerName || 'not signed in'}`"
      :title="playerName || 'Not signed in'"
      :aria-current="view === 'account' ? 'page' : undefined"
      @click="view = 'account'"
    >
      <Picture :picture="headUrl(playerName)" :size="32"><span class="avatar" aria-hidden="true">{{ initials(playerName) }}</span></Picture>
      <span class="side-text" style="min-width: 0">
        <span class="side-name mc-name">{{ playerName || "Not signed in" }}</span>
        <span class="side-sub">{{ playerName ? "Microsoft account" : "Sign in to play" }}</span>
      </span>
    </button>
  </nav>
</template>
