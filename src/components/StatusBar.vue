<script setup lang="ts">
import { computed } from "vue";
import { count, stageText } from "../logic/format";
import { closeFailure, lastPlayed, liveText, op, playerName, playing, trailing, view } from "../logic/operation";
import AppButton from "./AppButton.vue";
import Icon from "./Icon.vue";
import ProgressBar from "./ProgressBar.vue";
import Spinner from "./Spinner.vue";

const text = computed(() => trailing(view.value, op.value, playing.value, { playerName: playerName.value, lastPlayed: lastPlayed.value }));
const live = computed(() => liveText(op.value, playing.value));
const working = computed(() => op.value.phase === "preparing" || op.value.phase === "starting");
</script>

<template>
  <footer class="status" :class="{ 'is-failed': op.phase === 'failed' }">
    <span class="sr-only" role="status" aria-live="polite">{{ live }}</span>
    <Icon v-if="op.phase === 'failed'" name="warning" :size="16" class="warn" />
    <Spinner v-else-if="working" />
    <span v-else class="dot" :class="{ 'is-playing': playing.length > 0 }" aria-hidden="true" />
    <span v-if="op.phase === 'failed'" class="sr-only" role="alert">{{ text.lead }} {{ text.muted }}</span>
    <span class="status-text" :aria-hidden="op.phase === 'failed' || undefined">
      <span class="status-lead">{{ text.lead }}</span>
      <span v-if="text.muted" class="status-muted" :class="{ truncate: op.phase !== 'failed' }">{{ text.muted }}</span>
    </span>
    <template v-if="op.phase === 'preparing' && op.step > 0">
      <ProgressBar :value="op.done" :max="Math.max(op.total, 1)" :label="stageText(op.stage)" />
      <span class="tabular">{{ count(op.done) }} of {{ count(op.total) }} files</span>
    </template>
    <AppButton v-if="op.phase === 'failed'" variant="secondary" size="small" @click="closeFailure">Close</AppButton>
  </footer>
</template>
