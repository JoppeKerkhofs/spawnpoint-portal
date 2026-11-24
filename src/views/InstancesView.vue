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
import { computed, nextTick, onMounted, watch } from "vue";
import { bedrockInstalled, bedrockStart, instanceCreate, instanceLaunch, instances, loaderVersions, minecraftVersions, openInstanceFolder, panels, type Instance, type MinecraftVersion } from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import Card from "../components/Card.vue";
import EmptyState from "../components/EmptyState.vue";
import Icon from "../components/Icon.vue";
import KindBadge from "../components/KindBadge.vue";
import Note from "../components/Note.vue";
import Skeleton from "../components/Skeleton.vue";
import StatePill from "../components/StatePill.vue";
import Picture from "../components/Picture.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { errorText, minecraftText } from "../logic/format";
import { canCreate, loaderChoice, versionChoices, instancesState, instancesSummary, kindOf, kindTone, loaderText, playButton, sourceText } from "../logic/instances";
import { look } from "../logic/look";
import { os } from "../logic/os";
import { focusRow, instanceRowKey, takeReturn } from "../logic/focus";
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
const game = ref<"java" | "horizon">("java");
const allVersions = ref<MinecraftVersion[] | null>(null);
const snapshots = ref(false);
const versions = computed(() => versionChoices(allVersions.value, snapshots.value));
watch(versions, (list) => {
  if (list && !list.includes(version.value)) version.value = list[0] ?? "";
});
const loaderVersion = ref("");
const loaderList = ref<string[] | null>(null);
const loaderError = ref<CommandError | null>(null);
// A slow answer for an earlier choice must not fill the list of the current one.
let loaderAsk = 0;
watch([version, loader], async ([minecraft, kind]) => {
  const ask = ++loaderAsk;
  loaderVersion.value = "";
  loaderList.value = null;
  loaderError.value = null;
  if (kind === "none" || !minecraft) return;
  try {
    const list = await loaderVersions(minecraft, kind);
    if (ask === loaderAsk) loaderList.value = list;
  } catch (e) {
    if (ask === loaderAsk) loaderError.value = e as CommandError;
  }
});
const versionError = ref<CommandError | null>(null);
const creating = ref(false);
const createError = ref<CommandError | null>(null);
const nameInput = ref<HTMLInputElement>();
const newButton = ref<{ $el: HTMLElement }>();

async function loadVersions() {
  versionError.value = null;
  try {
    allVersions.value = await minecraftVersions();
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
  const horizon = game.value === "horizon";
  if (creating.value || !canCreate(name.value, versions.value, horizon)) return;
  creating.value = true;
  createError.value = null;
  try {
    await instanceCreate(name.value.trim(), horizon ? "" : version.value, horizon ? "horizon" : loaderChoice(loader.value, loaderVersion.value));
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

const back = takeReturn();
const bedrock = ref<string | null>(null);

async function playBedrock() {
  actionError.value = null;
  try {
    await bedrockStart();
  } catch (e) {
    actionError.value = e as CommandError;
  }
}

onMounted(async () => {
  // Not finding Bedrock is not an error: the card just stays away.
  bedrockInstalled().then((version) => (bedrock.value = version), () => {});
  focusRow(back);
  await load();
  focusRow(back);
});
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
          <label class="label" for="instance-game">Game</label>
          <select id="instance-game" v-model="game" class="select w-full">
            <option value="java">Java Edition</option>
            <option v-if="os === 'windows'" value="horizon">Horizon</option>
          </select>
        </div>
        <div v-if="game === 'java'" class="min-w-0 flex-[1_1_200px]">
          <div class="mb-1 flex items-center justify-between gap-3">
            <label class="label mb-0!" for="instance-version">Minecraft version</label>
            <label class="flex items-center gap-1.5 text-xs whitespace-nowrap">
              <input v-model="snapshots" type="checkbox" class="check" />
              Snapshots
            </label>
          </div>
          <select id="instance-version" v-model="version" class="select w-full" :disabled="!versions">
            <option v-if="!versions" value="" disabled>{{ versionError ? "Unavailable" : "Loading…" }}</option>
            <option v-for="v in versions ?? []" :key="v" :value="v">{{ v }}</option>
          </select>
        </div>
        <div v-if="game === 'java'" class="min-w-0 flex-[1_1_140px]">
          <label class="label" for="instance-loader">Mod loader</label>
          <select id="instance-loader" v-model="loader" class="select w-full">
            <option value="none">None</option>
            <option value="fabric">Fabric</option>
            <option value="neoforge">NeoForge</option>
            <option value="forge">Forge</option>
          </select>
        </div>
        <div v-if="game === 'java'" class="min-w-0 flex-[1_1_160px]">
          <label class="label" for="instance-loader-version">Loader version</label>
          <select id="instance-loader-version" v-model="loaderVersion" class="select w-full" :disabled="!loaderList">
            <option value="">{{ loader !== "none" && !loaderList && !loaderError ? "Loading…" : "Newest for this version" }}</option>
            <option v-for="v in loaderList ?? []" :key="v" :value="v">{{ v }}</option>
          </select>
        </div>
        <div class="flex gap-2">
          <AppButton variant="secondary" @click="closeForm">Cancel</AppButton>
          <AppButton variant="primary" type="submit" :disabled="creating || !canCreate(name, versions, game === 'horizon')">Create</AppButton>
        </div>
      </div>
      <div v-if="versionError" class="px-3.5 pb-3">
        <Note kind="err" role="alert">
          <strong>Couldn’t load the Minecraft versions.</strong>
          <div>{{ errorText(versionError) }}</div>
          <AppButton variant="secondary" size="small" class="mt-2" @click="loadVersions">Try again</AppButton>
        </Note>
      </div>
      <div v-if="loaderError" class="px-3.5 pb-3">
        <Note kind="err" role="alert">
          <strong>Couldn’t load the loader versions.</strong>
          <div>{{ errorText(loaderError) }}</div>
        </Note>
      </div>
      <div v-if="createError" class="px-3.5 pb-3">
        <Note kind="err" role="alert">
          <strong>Couldn’t create the instance.</strong>
          <div>{{ errorText(createError) }}</div>
        </Note>
      </div>
      <p v-if="game === 'horizon'" class="m-0 px-3.5 pb-3 text-xs text-(--muted)">
        Horizon is Minecraft: Java Edition rewritten in Rust. Portal downloads it on the first Play and prepares the game files, which takes a few minutes once.
      </p>
      <p v-else class="m-0 px-3.5 pb-3 text-xs text-(--muted)">Minecraft alpha and beta versions aren’t supported.</p>
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
      <div v-for="i in list!.instances" :key="i.id" class="row row-link" :data-row="instanceRowKey(i.id)" @click="openInstance(i.id)">
        <Picture :picture="i.picture" :size="blocky ? 34 : 32">
          <span v-if="blocky" class="flex w-[34px] shrink-0 items-center justify-center">
            <BlockCube :texture="instanceTexture(i)" :size="26" />
          </span>
          <Tile v-else :name="i.name" :texture="instanceTexture(i)" />
        </Picture>
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

    <Card v-if="bedrock">
      <div class="row">
        <span v-if="blocky" class="flex w-[34px] shrink-0 items-center justify-center"><BlockCube texture="stone" :size="26" /></span>
        <Tile v-else name="Minecraft Bedrock" texture="stone" />
        <div class="min-w-0 flex-1">
          <div class="mc-name truncate text-[15px] font-bold">Minecraft Bedrock</div>
          <div class="text-xs text-(--muted)">From the Microsoft Store. Portal only starts it.</div>
        </div>
        <div class="w-[190px] min-w-0 shrink-0 truncate font-semibold">Bedrock {{ bedrock }}</div>
        <span class="w-[160px] shrink-0 min-[1000px]:w-[230px]" />
        <div class="flex w-[150px] shrink-0 justify-end">
          <AppButton variant="primary" size="small" icon="play" aria-label="Play Minecraft Bedrock" @click="playBedrock">Play</AppButton>
        </div>
      </div>
    </Card>
  </div>
</template>
