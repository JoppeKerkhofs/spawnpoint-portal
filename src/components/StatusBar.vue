<script setup lang="ts">
import { computed } from "vue";
import { count, stageText } from "../logic/format";
import { availableVersion, update } from "../logic/update";
import { closeFailure, lastPlayed, liveText, op, playerName, playing, showLog, stopped, trailing, view } from "../logic/operation";
import AppButton from "./AppButton.vue";
import Icon from "./Icon.vue";
import ProgressBar from "./ProgressBar.vue";
import Spinner from "./Spinner.vue";

// Shown only when nothing else is going on; a new game or task takes the bar back.
const crashed = computed(() => (op.value.phase === "idle" && playing.value.length === 0 ? stopped.value : null));
const text = computed(() =>
  trailing(view.value, op.value, playing.value, { playerName: playerName.value, lastPlayed: lastPlayed.value, stopped: crashed.value?.name }),
);
const live = computed(() => liveText(op.value, playing.value));
const version = computed(() => availableVersion(update.value));
const working = computed(() => op.value.phase === "preparing" || op.value.phase === "starting");
const alerting = computed(() => op.value.phase === "failed" || crashed.value !== null);
</script>

<template>
  <footer class="status" :class="{ 'is-failed': op.phase === 'failed' }">
    <span class="sr-only" role="status" aria-live="polite">{{ live }}</span>
    <Icon v-if="alerting" name="warning" :size="16" class="warn" />
    <Spinner v-else-if="working" />
    <span v-else class="dot" :class="{ 'is-playing': playing.length > 0 }" aria-hidden="true" />
    <span v-if="alerting" class="sr-only" role="alert">{{ text.lead }} {{ text.muted }}</span>
    <span class="status-text" :aria-hidden="alerting || undefined">
      <span class="status-lead">{{ text.lead }}</span>
      <span v-if="text.muted" class="status-muted" :class="{ truncate: op.phase !== 'failed' }">{{ text.muted }}</span>
    </span>
    <template v-if="op.phase === 'preparing' && op.step > 0">
      <ProgressBar :value="op.done" :max="Math.max(op.total, 1)" :label="stageText(op.stage)" />
      <span class="tabular">{{ count(op.done) }} of {{ count(op.total) }} files</span>
    </template>
    <template v-if="crashed">
      <AppButton variant="secondary" size="small" @click="showLog(crashed.instance)">Show log</AppButton>
      <AppButton variant="secondary" size="small" @click="stopped = null">Close</AppButton>
    </template>
    <AppButton v-if="version && op.phase !== 'failed'" variant="secondary" size="small" @click="view = 'account'">Version {{ version }} is available</AppButton>
    <AppButton v-if="op.phase === 'failed'" variant="secondary" size="small" @click="closeFailure">Close</AppButton>
  </footer>
</template>
