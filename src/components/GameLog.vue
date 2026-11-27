<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { instanceLog, type CommandError, type LogSection } from "../api";
import { errorText } from "../logic/format";
import { playing } from "../logic/operation";
import AppButton from "./AppButton.vue";
import Note from "./Note.vue";

const props = defineProps<{ id: string }>();
defineEmits<{ back: [] }>();

const sections = ref<LogSection[] | null>(null);
const error = ref<CommandError | null>(null);
const copied = ref(false);
const isPlaying = computed(() => playing.value.some((p) => p.instance === props.id));

async function load() {
  copied.value = false;
  try {
    sections.value = await instanceLog(props.id);
    error.value = null;
  } catch (e) {
    error.value = e as CommandError;
  }
}

async function copy() {
  const text = (sections.value ?? []).map((s) => `== ${s.name} ==\n${s.text}`).join("\n\n");
  copied.value = await navigator.clipboard.writeText(text).then(() => true, () => false);
}

// The log is complete once the game has closed.
watch(isPlaying, load);
onMounted(load);
</script>

<template>
  <section class="card" aria-labelledby="log-heading">
    <div class="card-head flex-wrap">
      <h2 id="log-heading" class="card-title mc-name">Log</h2>
      <span class="card-meta">{{ isPlaying ? "The game is running." : "From the last time this instance was started." }}</span>
      <span class="flex-1" />
      <span v-if="copied" class="card-meta" role="status">Copied</span>
      <AppButton variant="secondary" size="small" :disabled="!sections?.length" @click="copy">Copy</AppButton>
      <AppButton variant="secondary" size="small" @click="load">Refresh</AppButton>
      <AppButton variant="secondary" size="small" icon="chevron-left" @click="$emit('back')">Back to mods</AppButton>
    </div>
    <div v-if="error" class="px-4 pb-4" role="alert">
      <Note kind="err">
        <strong>Couldn’t read the log.</strong>
        <div>{{ errorText(error) }}</div>
      </Note>
    </div>
    <div v-else-if="sections?.length === 0" class="row justify-center text-(--muted)">Nothing yet. The log appears once this instance has been started.</div>
    <template v-for="s in sections ?? []" :key="s.name">
      <div class="row mono font-bold">{{ s.name }}</div>
      <pre class="mono m-0 max-h-[26rem] overflow-auto px-4 pb-3 text-xs whitespace-pre-wrap break-all" tabindex="0" :aria-label="s.name">{{ s.text }}</pre>
    </template>
  </section>
</template>
