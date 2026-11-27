<script setup lang="ts">
import { computed, onMounted, ref, useId, watch } from "vue";
import { loaderVersions, minecraftVersions, type CommandError, type MinecraftVersion } from "../api";
import { errorText } from "../logic/format";
import { versionChoices } from "../logic/instances";
import AppButton from "./AppButton.vue";
import Note from "./Note.vue";

// The Minecraft version, the mod loader and its version; `ready` once there is a version to choose.
const version = defineModel<string>("version", { required: true });
const loader = defineModel<string>("loader", { required: true });
const loaderVersion = defineModel<string>("loaderVersion", { required: true });
const ready = defineModel<boolean>("ready", { default: false });

const uid = useId();
const allVersions = ref<MinecraftVersion[] | null>(null);
const snapshots = ref(false);
const versions = computed(() => versionChoices(allVersions.value, snapshots.value));
const versionError = ref<CommandError | null>(null);
const loaderList = ref<string[] | null>(null);
const loaderError = ref<CommandError | null>(null);

// A version given from outside that is a snapshot shows only with snapshots on.
watch(allVersions, (all) => {
  if (all?.find((v) => v.id === version.value)?.snapshot) snapshots.value = true;
});
watch(versions, (list) => {
  if (list && !list.includes(version.value)) version.value = list[0] ?? "";
  ready.value = !!list?.length;
});

// A slow answer for an earlier choice must not fill the list of the current one.
let loaderAsk = 0;
watch(
  [version, loader],
  async ([minecraft, kind], before) => {
    const ask = ++loaderAsk;
    // The first run keeps a loader version given from outside.
    if (before) loaderVersion.value = "";
    loaderList.value = null;
    loaderError.value = null;
    if (kind === "none" || !minecraft) return;
    try {
      const list = await loaderVersions(minecraft, kind);
      if (ask === loaderAsk) loaderList.value = list;
    } catch (e) {
      if (ask === loaderAsk) loaderError.value = e as CommandError;
    }
  },
  { immediate: true },
);

async function loadVersions() {
  versionError.value = null;
  try {
    allVersions.value = await minecraftVersions();
  } catch (e) {
    versionError.value = e as CommandError;
  }
}

onMounted(loadVersions);
</script>

<template>
  <div class="min-w-0 flex-[1_1_200px]">
    <div class="mb-1 flex items-center justify-between gap-3">
      <label class="label mb-0!" :for="`${uid}-version`">Minecraft version</label>
      <label class="flex items-center gap-1.5 text-xs whitespace-nowrap">
        <input v-model="snapshots" type="checkbox" class="check" />
        Snapshots
      </label>
    </div>
    <select :id="`${uid}-version`" v-model="version" class="select w-full" :disabled="!versions">
      <option v-if="!versions" value="" disabled>{{ versionError ? "Unavailable" : "Loading…" }}</option>
      <option v-for="v in versions ?? []" :key="v" :value="v">{{ v }}</option>
    </select>
  </div>
  <div class="min-w-0 flex-[1_1_140px]">
    <label class="label" :for="`${uid}-loader`">Mod loader</label>
    <select :id="`${uid}-loader`" v-model="loader" class="select w-full">
      <option value="none">None</option>
      <option value="fabric">Fabric</option>
      <option value="neoforge">NeoForge</option>
      <option value="forge">Forge</option>
    </select>
  </div>
  <div class="min-w-0 flex-[1_1_160px]">
    <label class="label" :for="`${uid}-loader-version`">Loader version</label>
    <select :id="`${uid}-loader-version`" v-model="loaderVersion" class="select w-full" :disabled="!loaderList">
      <option value="">{{ loader !== "none" && !loaderList && !loaderError ? "Loading…" : "Newest for this version" }}</option>
      <option v-for="v in loaderList ?? []" :key="v" :value="v">{{ v }}</option>
    </select>
  </div>
  <Note v-if="versionError" kind="err" role="alert" class="basis-full">
    <strong>Couldn’t load the Minecraft versions.</strong>
    <div>{{ errorText(versionError) }}</div>
    <AppButton variant="secondary" size="small" class="mt-2" @click="loadVersions">Try again</AppButton>
  </Note>
  <Note v-if="loaderError" kind="err" role="alert" class="basis-full">
    <strong>Couldn’t load the loader versions.</strong>
    <div>{{ errorText(loaderError) }}</div>
  </Note>
</template>
