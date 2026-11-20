<script lang="ts">
import { ref } from "vue";
import type { CommandError, Panel } from "../api";
import { instanceList as list } from "../logic/instances";

// Outside the component, so leaving and re-entering the view shows the earlier answer at once.
const knownPanels = ref<Panel[]>([]);
const loading = ref(false);
const loadError = ref<CommandError | null>(null);
</script>

<script setup lang="ts">
import { computed, nextTick, onMounted } from "vue";
import { instanceCreate, instanceLaunch, instances, minecraftVersions, openInstanceFolder, panels, type Instance } from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import Card from "../components/Card.vue";
import EmptyState from "../components/EmptyState.vue";
import Icon from "../components/Icon.vue";
import KindBadge from "../components/KindBadge.vue";
import Note from "../components/Note.vue";
import Skeleton from "../components/Skeleton.vue";
import StatePill from "../components/StatePill.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { errorText, minecraftText } from "../logic/format";
import { canCreate, instancesState, instancesSummary, kindOf, kindTone, loaderText, playButton, sourceText } from "../logic/instances";
import { look } from "../logic/look";
import { markPlaying, op, openInstance, playing, view, withProgress } from "../logic/operation";
import { instanceTexture } from "../logic/visuals";

const state = computed(() => instancesState(list.value));
const summary = computed(() => instancesSummary(list.value));
const opKey = computed(() => (op.value.phase === "idle" ? null : (op.value.key ?? null)));
const blocky = computed(() => look.value.theme === "blocky");
const skeletonRow: Array<[string, string]> = [["32px", "32px"], ["flex", "14px"], ["120px", "14px"], ["140px", "30px"]];

const actionError = ref<CommandError | null>(null);

async function load() {
  if (loading.value) return;
  loading.value = true;
  try {
    const [l, p] = await Promise.all([instances(), panels().catch(() => knownPanels.value)]);
    knownPanels.value = p;
    list.value = l;
    loadError.value = null;
  } catch (e) {
    loadError.value = e as CommandError;
  } finally {
    loading.value = false;
  }
}

const formOpen = ref(false);
const name = ref("");
const version = ref("");
const loader = ref("none");
const versions = ref<string[] | null>(null);
const versionError = ref<CommandError | null>(null);
const creating = ref(false);
const createError = ref<CommandError | null>(null);
const nameInput = ref<HTMLInputElement>();
const newButton = ref<{ $el: HTMLElement }>();

async function loadVersions() {
  versionError.value = null;
  try {
    versions.value = await minecraftVersions();
    if (!versions.value.includes(version.value)) version.value = versions.value[0] ?? "";
  } catch (e) {
    versionError.value = e as CommandError;
  }
}

function closeForm() {
  formOpen.value = false;
  name.value = "";
  nextTick(() => newButton.value?.$el.focus());
}

function toggleForm() {
  if (formOpen.value) return closeForm();
  formOpen.value = true;
  createError.value = null;
  if (!versions.value) loadVersions();
  nextTick(() => nameInput.value?.focus());
}

async function create() {
  if (creating.value || !canCreate(name.value, versions.value)) return;
  creating.value = true;
  createError.value = null;
  try {
    await instanceCreate(name.value.trim(), version.value, loader.value === "none" ? null : loader.value);
    closeForm();
    await load();
  } catch (e) {
    createError.value = e as CommandError;
  } finally {
    creating.value = false;
  }
}

const button = (i: Instance) => playButton(i, op.value, opKey.value, playing.value);
const isPlaying = (i: Instance) => playing.value.some((p) => p.instance === i.id);

async function play(i: Instance) {
  actionError.value = null;
  const launched = await withProgress(i.name, () => instanceLaunch(i.id), i.id);
  if (launched) markPlaying(launched);
}

async function showFolder(folder: string) {
  actionError.value = null;
  try {
    await openInstanceFolder(folder);
  } catch (e) {
    actionError.value = e as CommandError;
  }
}

onMounted(load);
</script>

<template>
  <ViewHead title="Instances" :summary="summary">
    <template #actions>
      <AppButton ref="newButton" variant="primary" icon="plus" :aria-expanded="formOpen" @click="toggleForm">New instance</AppButton>
    </template>
  </ViewHead>
  <div class="content">
    <form v-if="formOpen" class="card" aria-labelledby="new-instance-title" @submit.prevent="create">
      <div class="card-head">
        <h2 id="new-instance-title" class="card-title mc-name">New instance</h2>
        <span class="card-meta">A plain game setup you manage yourself.</span>
      </div>
      <div class="card-body flex flex-wrap items-end gap-3">
        <div class="min-w-0 flex-[2_1_200px]">
          <label class="label" for="instance-name">Name</label>
          <input id="instance-name" ref="nameInput" v-model="name" class="input w-full" placeholder="My new world" />
        </div>
        <div class="min-w-0 flex-[1_1_140px]">
          <label class="label" for="instance-version">Minecraft version</label>
          <select id="instance-version" v-model="version" class="select w-full" :disabled="!versions">
            <option v-if="!versions" value="" disabled>{{ versionError ? "Unavailable" : "Loading…" }}</option>
            <option v-for="v in versions ?? []" :key="v" :value="v">{{ v }}</option>
          </select>
        </div>
        <div class="min-w-0 flex-[1_1_140px]">
          <label class="label" for="instance-loader">Mod loader</label>
          <select id="instance-loader" v-model="loader" class="select w-full">
            <option value="none">None</option>
            <option value="fabric">Fabric</option>
            <option value="neoforge">NeoForge</option>
            <option value="forge">Forge</option>
          </select>
        </div>
        <div class="min-w-0 flex-[1_1_160px]">
          <label class="label" for="instance-loader-version">Loader version</label>
          <select id="instance-loader-version" class="select w-full" :disabled="loader === 'none'">
            <option>Newest for this version</option>
          </select>
        </div>
        <div class="flex gap-2">
          <AppButton variant="secondary" @click="closeForm">Cancel</AppButton>
          <AppButton variant="primary" type="submit" :disabled="creating || !canCreate(name, versions)">Create</AppButton>
        </div>
      </div>
      <div v-if="versionError" class="px-3.5 pb-3">
        <Note kind="err" role="alert">
          <strong>Couldn’t load the Minecraft versions.</strong>
          <div>{{ errorText(versionError) }}</div>
          <AppButton variant="secondary" size="small" class="mt-2" @click="loadVersions">Try again</AppButton>
        </Note>
      </div>
      <div v-if="createError" class="px-3.5 pb-3">
        <Note kind="err" role="alert">
          <strong>Couldn’t create the instance.</strong>
          <div>{{ errorText(createError) }}</div>
        </Note>
      </div>
      <p class="m-0 px-3.5 pb-3 text-xs text-(--muted)">Minecraft alpha and beta versions aren’t supported.</p>
    </form>

    <Note v-if="actionError" kind="err" role="alert">{{ errorText(actionError) }}</Note>

    <Note v-if="loadError && !list" kind="err" role="alert">
      <strong>Couldn’t read your instances.</strong>
      <div>{{ errorText(loadError) }}</div>
      <AppButton variant="secondary" size="small" class="mt-2" :disabled="loading" @click="load">Try again</AppButton>
    </Note>

    <Card v-else-if="state === 'loading'" aria-busy="true">
      <div class="px-4"><Skeleton :rows="[skeletonRow, skeletonRow, skeletonRow]" /></div>
    </Card>

    <EmptyState v-else-if="state === 'empty'" title="No instances yet" :max-width="420">
      Joining a server makes one for you automatically. You can also create a plain one with the button above.
      <template #action>
        <AppButton variant="primary" icon="server" @click="view = 'servers'">Go to Servers</AppButton>
      </template>
    </EmptyState>

    <Card v-else>
      <div class="card-head text-xs font-bold">
        <span class="flex-1">Name</span>
        <span class="w-[190px] shrink-0">Version</span>
        <span class="w-[160px] shrink-0 min-[1000px]:w-[230px]">Comes from</span>
        <span class="w-[150px] shrink-0" />
      </div>
      <div v-for="i in list!.instances" :key="i.id" class="row row-link" @click="openInstance(i.id)">
        <span v-if="blocky" class="flex w-[34px] shrink-0 items-center justify-center">
          <BlockCube :texture="instanceTexture(i)" :size="26" />
        </span>
        <Tile v-else :name="i.name" :texture="instanceTexture(i)" />
        <div class="min-w-0 flex-1">
          <button type="button" class="mc-name block max-w-full truncate text-left text-[15px] font-bold" :title="i.name" @click="openInstance(i.id)">
            {{ i.name }}
          </button>
          <StatePill v-if="isPlaying(i)" tone="ok" class="mt-[3px]">Game running</StatePill>
        </div>
        <div class="w-[190px] min-w-0 shrink-0">
          <div class="truncate font-semibold">{{ minecraftText(i.minecraft) }}</div>
          <div class="truncate text-xs text-(--muted)" :title="loaderText(i.loader)">{{ loaderText(i.loader) }}</div>
        </div>
        <div class="w-[160px] min-w-0 shrink-0 min-[1000px]:w-[230px]">
          <KindBadge :kind="kindTone(i)">{{ kindOf(i) }}</KindBadge>
          <div class="mt-[3px] truncate text-xs text-(--muted)" :title="sourceText(i, knownPanels)">{{ sourceText(i, knownPanels) }}</div>
        </div>
        <div class="flex w-[150px] shrink-0 justify-end gap-1.5" @click.stop>
          <AppButton variant="secondary" size="small" :aria-label="`Open ${i.name}`" @click="openInstance(i.id)">Open</AppButton>
          <AppButton variant="primary" size="small" icon="play" :disabled="button(i).disabled" :aria-label="button(i).aria" @click="play(i)">{{
            button(i).label
          }}</AppButton>
        </div>
      </div>
      <div v-for="u in list!.unreadable" :key="u.folder" class="row">
        <span class="flex w-[34px] shrink-0 items-center justify-center text-(--destructive)"><Icon name="warning" :size="20" /></span>
        <div class="min-w-0 flex-1">
          <div class="mono truncate font-bold" :title="`instances\\${u.folder}`">instances\{{ u.folder }}</div>
          <div class="text-xs text-(--muted)">Portal can’t read this instance’s settings, so it can’t play it. The files are still there.</div>
        </div>
        <AppButton variant="secondary" size="small" icon="folder" @click="showFolder(u.folder)">Show folder</AppButton>
      </div>
    </Card>
  </div>
</template>
