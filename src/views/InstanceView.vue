<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  defaults,
  instanceAddMod,
  instanceCheckManual,
  instanceDelete,
  instanceDuplicate,
  instanceGet,
  instanceLaunch,
  instanceMods,
  instanceRemoveMod,
  instanceRename,
  instanceSetJavaArgs,
  instanceSetMemory,
  instanceSetPicture,
  instanceSetVersion,
  modsInstall,
  modsInstalled,
  openModsFolder,
  openPage,
  packUpdate,
  packVersions,
  panels,
  type CommandError,
  type ContentKind,
  type Defaults,
  type Instance,
  type InstalledMod,
  type ModFile,
  type PackVersion,
  type Panel,
} from "../api";
import { packVersionText } from "../logic/packs";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import GameLog from "../components/GameLog.vue";
import Icon from "../components/Icon.vue";
import KindBadge from "../components/KindBadge.vue";
import ModBrowser from "../components/ModBrowser.vue";
import Note from "../components/Note.vue";
import SegFilter from "../components/SegFilter.vue";
import Skeleton from "../components/Skeleton.vue";
import Picture from "../components/Picture.vue";
import Tile from "../components/Tile.vue";
import VersionFields from "../components/VersionFields.vue";
import ViewHead from "../components/ViewHead.vue";
import { errorText, lastPlayedText, megabytes, memoryLabel } from "../logic/format";
import { dropInstance, isHorizon, loaderChoice, playButton } from "../logic/instances";
import { look } from "../logic/look";
import { pictureFromFile } from "../logic/picture";
import {
  deleteBody,
  browseNote,
  filterMods,
  handHeading,
  installedSentence,
  instanceSummary,
  isHttps,
  kindWords,
  memoryOptions,
  modCounts,
  modsText,
  noModsText,
  orderMods,
  skippedName,
  modIcon,
  modPage,
  modSource,
  updatableMod,
  type ModFilter,
} from "../logic/mods";
import { instanceRowKey, leave } from "../logic/focus";
import { afterJoin, busy, lastPlayed, markPlaying, op, openInstance, openInstanceId, openLog, playing, withProgress } from "../logic/operation";
import { instanceTexture } from "../logic/visuals";

const id = openInstanceId.value ?? "";
const instance = ref<Instance | null>(null);
const mods = ref<ModFile[] | null>(null);
// What the files card shows: the mods, the resource packs or the shaders.
const kind = ref<ContentKind>("mod");
const words = computed(() => kindWords[kind.value]);
const kinds = (Object.keys(kindWords) as ContentKind[]).map((k) => ({ value: k, label: kindWords[k].title }));
const knownPanels = ref<Panel[]>([]);
const choices = ref<Defaults | null>(null);
const loadError = ref<CommandError | null>(null);
const modsError = ref<CommandError | null>(null);
const actionError = ref<CommandError | null>(null);
const checkedClear = ref(false);
const filter = ref<ModFilter>("all");
const query = ref("");
const removing = ref<ModFile | null>(null);
const deleting = ref(false);
const browsing = ref(false);
const pictureInput = ref<HTMLInputElement | null>(null);
const browseButton = ref<InstanceType<typeof AppButton> | null>(null);
const known = ref<InstalledMod[] | null>(null);
const modsNote = ref<{ text: string; kind: "info" | "warn" } | null>(null);

const skeletonRow: Array<[string, string]> = [["flex", "14px"], ["64px", "14px"], ["70px", "18px"], ["96px", "28px"]];
const blocky = computed(() => look.value.theme === "blocky");
const opKey = computed(() => (op.value.phase === "idle" ? null : (op.value.key ?? null)));
const isPlaying = computed(() => playing.value.some((p) => p.instance === id));
const play = computed(() => (instance.value ? playButton(instance.value, op.value, opKey.value, playing.value) : null));
const updatable = (m: ModFile) => updatableMod(m, known.value);
const counts = computed(() => modCounts(mods.value ?? []));
const shown = computed(() => filterMods(orderMods(mods.value ?? []), filter.value, query.value));
const filters = computed(() => [
  { value: "all", label: `All ${counts.value.all}` },
  { value: "pack", label: `Pack ${counts.value.pack}` },
  { value: "own", label: `Own ${counts.value.own}` },
]);
const memory = computed(() => instance.value?.memory_mb ?? choices.value?.default_memory_mb ?? null);
const memoryChoices = computed(() => (memory.value === null ? [] : memoryOptions(choices.value?.memory_choices_mb ?? [], memory.value)));

async function loadInstance() {
  try {
    instance.value = await instanceGet(id);
    loadError.value = null;
  } catch (e) {
    loadError.value = e as CommandError;
  }
}

// An answer for a kind the player has left meanwhile is dropped.
async function loadMods() {
  const k = kind.value;
  try {
    const list = await instanceMods(id, k);
    if (k !== kind.value) return;
    mods.value = list;
    modsError.value = null;
  } catch (e) {
    if (k === kind.value) modsError.value = e as CommandError;
  }
}

// An extra after the list: without Modrinth the list is the same, only without its buttons.
async function loadKnown() {
  const k = kind.value;
  const list = await modsInstalled(id, k).catch(() => null);
  if (k === kind.value) known.value = list;
}

watch(kind, () => {
  mods.value = known.value = modsError.value = modsNote.value = null;
  browsing.value = false;
  query.value = "";
  filter.value = "all";
  refresh();
});

const refresh = async () => {
  await loadMods();
  await loadKnown();
};

async function installMod(project: string) {
  actionError.value = null;
  modsNote.value = null;
  const done = await withProgress(instance.value!.name, () => modsInstall(id, kind.value, project), `mod:${project}`);
  if (done) modsNote.value = { text: installedSentence(done), kind: done.failed.length ? "warn" : "info" };
  await refresh();
}

// Only a Modrinth pack installed without a server: a server's sync decides its own pack version.
const pack = computed(() => (instance.value?.source.kind === "pack" && instance.value.source.pack.catalogue === "modrinth" ? instance.value.source.pack : null));
const packList = ref<PackVersion[] | null>(null);
const packChoice = ref("");

// An extra: without Modrinth the page works, only without the version choice.
async function loadPackVersions() {
  if (!pack.value) return;
  packChoice.value = pack.value.version;
  packList.value = await packVersions(pack.value.project, pack.value.version).catch(() => null);
}

async function changePack() {
  const v = packList.value?.find((x) => x.id === packChoice.value);
  if (!v) return;
  actionError.value = null;
  const changed = await withProgress(instance.value!.name, () => packUpdate(id, v.id, v.name), id);
  if (changed) instance.value = changed;
  else await loadInstance();
  await refresh();
}

const versionOpen = ref(false);
// The name as it is typed in the title; null while the title is not being edited.
const newName = ref<string | null>(null);
const nameInput = ref<HTMLInputElement | null>(null);
const renameButton = ref<InstanceType<typeof AppButton> | null>(null);
const copying = ref(false);
const version = ref("");
const loader = ref("none");
const loaderVersion = ref("");
const versionsReady = ref(false);
// Only an instance the player made: a sync decides the version of a server's or a pack's.
const ownVersion = computed(() => instance.value?.source.kind === "manual" && !isHorizon(instance.value.loader));

function toggleVersion() {
  versionOpen.value = !versionOpen.value;
  const i = instance.value;
  if (!versionOpen.value || !i) return;
  version.value = i.minecraft;
  loader.value = i.loader?.kind ?? "none";
  loaderVersion.value = i.loader?.version ?? "";
}

async function editName() {
  newName.value = instance.value!.name;
  await nextTick();
  nameInput.value?.select();
}

// Enter and leaving the field save. The keys give the focus back to the pencil; a click elsewhere keeps its own.
async function endName(save: boolean, refocus: boolean) {
  const name = newName.value?.trim();
  if (newName.value === null) return;
  newName.value = null;
  if (save && name && name !== instance.value!.name) await run(async () => void (instance.value = await instanceRename(id, name)));
  if (!refocus) return;
  await nextTick();
  (renameButton.value?.$el as HTMLElement | undefined)?.focus();
}

async function duplicate() {
  copying.value = true;
  await run(async () => openInstance((await instanceDuplicate(id, `${instance.value!.name} copy`)).id));
  copying.value = false;
}

const setVersion = () =>
  run(async () => {
    instance.value = await instanceSetVersion(id, version.value, loaderChoice(loader.value, loaderVersion.value));
    await loadKnown();
  });

async function closeBrowser() {
  browsing.value = false;
  await nextTick();
  (browseButton.value?.$el as HTMLElement | undefined)?.focus();
}

const fail = (e: unknown) => (actionError.value = e as CommandError);

async function setMemory(event: Event) {
  const select = event.target as HTMLSelectElement;
  actionError.value = null;
  try {
    instance.value = await instanceSetMemory(id, Number(select.value));
  } catch (e) {
    fail(e);
    select.value = String(memory.value);
  }
}

const javaArgs = ref("");
watch(() => instance.value?.java_args, (a) => (javaArgs.value = a ?? ""), { immediate: true });
const saveJavaArgs = () => run(async () => void (instance.value = await instanceSetJavaArgs(id, javaArgs.value)));

async function choosePicture(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  actionError.value = null;
  try {
    instance.value = await instanceSetPicture(id, await pictureFromFile(file));
  } catch (e) {
    fail(e instanceof Error ? { kind: "other", message: "That image could not be read." } : e);
  }
}

async function removePicture() {
  actionError.value = null;
  try {
    instance.value = await instanceSetPicture(id, null);
    await nextTick();
    pictureInput.value?.focus();
  } catch (e) {
    fail(e);
  }
}

async function launch() {
  actionError.value = null;
  const launched = await withProgress(instance.value!.name, () => instanceLaunch(id), id);
  if (launched) markPlaying(launched);
}

async function addMod() {
  actionError.value = null;
  try {
    if (await instanceAddMod(id, kind.value)) await refresh();
  } catch (e) {
    fail(e);
  }
}

async function confirmRemove() {
  const mod = removing.value;
  removing.value = null;
  if (!mod) return;
  actionError.value = null;
  try {
    await instanceRemoveMod(id, kind.value, mod.name);
  } catch (e) {
    fail(e);
  }
  await refresh();
}

async function confirmDelete() {
  deleting.value = false;
  actionError.value = null;
  try {
    await instanceDelete(id);
    dropInstance(id);
    leave(instanceRowKey(id), "instances");
  } catch (e) {
    fail(e);
  }
}

async function checkAgain() {
  actionError.value = null;
  try {
    instance.value = await instanceCheckManual(id);
    checkedClear.value = instance.value.manual.length === 0;
  } catch (e) {
    fail(e);
  }
}

const run = (action: () => Promise<void>) => {
  actionError.value = null;
  return action().catch(fail);
};

// "Last played" changes when the game closes.
watch(isPlaying, (now, before) => before && !now && loadInstance());
watch(instance, (i) => (lastPlayed.value = i ? lastPlayedText(i.name, i.last_played, new Date(), i.played_seconds) : null));

onMounted(() => {
  loadInstance().then(loadPackVersions);
  loadMods().then(loadKnown);
  panels().then((p) => (knownPanels.value = p), () => {});
  defaults().then((d) => (choices.value = d), () => {});
});
onUnmounted(() => {
  lastPlayed.value = null;
  afterJoin.value = null;
  // Another instance's log, asked for from the status bar, stays asked for.
  if (openLog.value === id) openLog.value = null;
});
</script>

<template>
  <ViewHead :title="instance?.name ?? 'Instance'" :summary="instance ? instanceSummary(instance, knownPanels) : ''">
    <template #back>
      <button type="button" class="inline-flex items-center gap-1 text-xs hover:underline" @click="leave(instanceRowKey(id), 'instances')">
        <Icon name="chevron-left" :size="14" />Instances
      </button>
    </template>
    <template v-if="newName !== null" #title>
      <input
        ref="nameInput"
        v-model="newName"
        class="input name-edit w-full"
        aria-label="Name"
        @keydown.enter="endName(true, true)"
        @keydown.esc="endName(false, true)"
        @blur="endName(true, false)"
      />
    </template>
    <template v-if="instance" #lead>
      <label class="pick-picture" title="Change picture">
        <input ref="pictureInput" type="file" accept="image/png,image/jpeg,image/webp" class="sr-only" aria-label="Change picture" @change="choosePicture" />
      <Picture :picture="instance.picture" :size="blocky ? 52 : 44">
        <span v-if="blocky" class="flex h-[52px] w-[52px] items-center justify-center">
          <BlockCube :texture="instanceTexture(instance)" :size="40" spin bob />
        </span>
        <Tile v-else :name="instance.name" :texture="instanceTexture(instance)" :size="44" />
      </Picture>
      </label>
    </template>
    <template v-if="instance && play" #actions>
      <div v-if="!isHorizon(instance.loader)" class="flex items-center gap-2">
        <label for="instance-memory" class="text-[15px] text-(--muted)">Memory</label>
        <select id="instance-memory" class="select w-[92px]" :value="memory ?? ''" :disabled="memory === null" @change="setMemory">
          <option v-for="mb in memoryChoices" :key="mb" :value="mb">{{ memoryLabel(mb) }}</option>
        </select>
      </div>
      <AppButton v-if="instance.own_picture" variant="secondary" @click="removePicture">Remove picture</AppButton>
      <AppButton ref="renameButton" variant="secondary" icon="pencil" aria-label="Rename" title="Rename" @click="editName" />
      <AppButton
        variant="secondary"
        icon="copy"
        :aria-label="copying ? 'Duplicating…' : 'Duplicate'"
        title="Duplicate: a copy with its mods, configs and worlds"
        :disabled="busy || isPlaying || copying"
        @click="duplicate"
      />
      <AppButton v-if="ownVersion" variant="secondary" :aria-expanded="versionOpen" @click="toggleVersion">Change version</AppButton>
      <AppButton variant="secondary" :aria-pressed="openLog === id" @click="openLog = openLog === id ? null : id">Log</AppButton>
      <AppButton variant="secondary" icon="folder" @click="run(() => openModsFolder(id, kind))">{{ words.title }} folder</AppButton>
      <AppButton variant="danger" icon="trash" :disabled="busy" @click="deleting = true">Delete</AppButton>
      <AppButton variant="primary" icon="play" :disabled="play.disabled" :aria-label="play.aria" @click="launch">{{ play.label }}</AppButton>
    </template>
  </ViewHead>
  <div class="content">
    <Note v-if="actionError" kind="err" role="alert">{{ errorText(actionError) }}</Note>
    <Note v-if="loadError && !instance" kind="err" role="alert">
      <strong>Couldn’t read this instance.</strong>
      <div>{{ errorText(loadError) }}</div>
      <AppButton variant="secondary" size="small" class="mt-2" @click="loadInstance">Try again</AppButton>
    </Note>

    <template v-if="instance">
      <Note v-if="instance.manual.length > 0" kind="warn">
        <section aria-labelledby="hand-heading" class="flex flex-col gap-2">
          <div>
            <h2 id="hand-heading" class="m-0 text-base font-bold">{{ handHeading(instance.manual.length) }}</h2>
            <div>
              Its author doesn’t allow launchers to download it. Get it from its page and put it in the mods folder. It disappears from this list once it’s there.
            </div>
          </div>
          <div v-for="f in instance.manual" :key="f.path" class="flex flex-wrap items-center gap-2.5">
            <span class="mono min-w-0 break-all font-bold">{{ f.name }}</span>
            <AppButton v-if="isHttps(f.page)" variant="secondary" size="small" icon="external" @click="run(() => openPage(f.page))">Open its page</AppButton>
          </div>
          <div class="flex gap-2">
            <AppButton variant="secondary" size="small" icon="folder" @click="run(() => openModsFolder(id, 'mod'))">Open mods folder</AppButton>
            <AppButton variant="secondary" size="small" @click="checkAgain">Check again</AppButton>
          </div>
        </section>
      </Note>
      <Note v-else-if="checkedClear" kind="info" icon="check" role="status" class="note-ok">All files are in place. {{ instance.name }} is ready to play.</Note>

      <Note v-if="afterJoin?.instance === instance.id" kind="info" role="note">{{ afterJoin.text }}</Note>

      <Note v-if="modsNote" :kind="modsNote.kind" role="status">{{ modsNote.text }}</Note>

      <Note v-for="path in instance.skipped" :key="path" kind="info" role="note" aria-label="Skipped file">
        The last sync left one pack file out: <span class="mono font-bold">{{ skippedName(path) }}</span>. Your own file with the same name is in its place, and Portal never
        touches your files. Remove yours if you want the pack’s version.
      </Note>

      <section v-if="versionOpen && ownVersion" class="card" aria-labelledby="version-heading">
        <div class="card-head">
          <h2 id="version-heading" class="card-title mc-name">Change version</h2>
        </div>
        <form class="row flex-wrap items-end gap-3" @submit.prevent="setVersion">
          <VersionFields v-model:version="version" v-model:loader="loader" v-model:loader-version="loaderVersion" v-model:ready="versionsReady" />
          <AppButton variant="secondary" type="submit" :disabled="busy || isPlaying || !versionsReady">Change version</AppButton>
          <p class="m-0 basis-full text-xs text-(--muted)">
            Your mods stay as they are. One made for another version or loader can stop the game from starting: update or remove it.
          </p>
        </form>
      </section>

      <section v-if="pack && packList" class="card" aria-label="Pack version">
        <div class="card-head flex-wrap">
          <label for="pack-version" class="card-title mc-name">Pack version</label>
          <span class="card-meta">Another version replaces the pack’s files. Your own mods and your worlds stay.</span>
          <span class="flex-1" />
          <select id="pack-version" v-model="packChoice" class="select" style="max-width: 320px; height: 30px">
            <option v-for="v in packList" :key="v.id" :value="v.id">{{ packVersionText(v) }}</option>
          </select>
          <AppButton variant="secondary" size="small" :disabled="busy || isPlaying" @click="changePack">
            {{ packChoice === pack.version ? "Check files again" : "Change version" }}
          </AppButton>
        </div>
      </section>

      <GameLog v-if="openLog === id" :id="id" @back="openLog = null" />
      <ModBrowser
        v-else-if="browsing && (kind !== 'mod' || instance.loader)"
        :id="id"
        :kind="kind"
        :minecraft="instance.minecraft"
        :loader="instance.loader"
        :installed="known"
        @install="installMod"
        @page="(url) => run(() => openPage(url))"
        @back="closeBrowser"
      />
      <section v-else class="card" aria-labelledby="mods-heading">
        <div class="card-head flex-wrap">
          <h2 id="mods-heading" class="card-title mc-name">{{ words.title }}</h2>
          <span class="card-meta tabular">{{ mods ? modsText(counts.all, kind) : "" }}</span>
          <SegFilter v-if="!isHorizon(instance.loader)" v-model="kind" :options="kinds" />
          <span class="flex-1" />
          <SegFilter v-model="filter" :options="filters" />
          <label for="mod-search" class="sr-only">Search {{ words.many }}</label>
          <input id="mod-search" v-model="query" class="input" style="width: 170px; height: 30px" :placeholder="`Search ${words.many}`" />
          <AppButton v-if="kind !== 'mod' || instance.loader" ref="browseButton" variant="secondary" size="small" icon="plus" @click="browsing = true">
            Browse {{ words.many }}
          </AppButton>
          <AppButton variant="secondary" size="small" icon="upload" @click="addMod">Add a {{ kind === "mod" ? "mod" : "zip" }} file</AppButton>
        </div>
        <div v-if="browseNote(instance, kind)" class="row text-xs text-(--muted)">{{ browseNote(instance, kind) }}</div>
        <div class="row gap-2 text-xs">
          <Icon name="lock" :size="14" class="shrink-0" />
          <span>
            Pack {{ words.many }} are managed by the sync and can change with the next pack update. Your own {{ words.many }} are never changed or removed by a
            sync.
          </span>
        </div>
        <div v-if="modsError && !mods" class="px-4 pb-4" role="alert">
          <Note kind="err">
            <strong>Couldn’t read the {{ words.many }}.</strong>
            <div>{{ errorText(modsError) }}</div>
            <AppButton variant="secondary" size="small" class="mt-2" @click="loadMods">Try again</AppButton>
          </Note>
        </div>
        <div v-else-if="!mods" class="px-4" aria-busy="true">
          <Skeleton :rows="[skeletonRow, skeletonRow, skeletonRow]" />
        </div>
        <template v-else>
          <div v-for="m in shown" :key="m.name" class="row" style="padding-block: 8px; min-height: 34px">
            <span class="flex h-8 w-8 shrink-0">
              <Picture :picture="modIcon(m, known)" :size="32"><Icon name="box" :size="32" class="text-(--muted)" /></Picture>
            </span>
            <span class="mono min-w-0 flex-1 truncate" :title="m.name">{{ m.name }}</span>
            <span class="tabular w-16 shrink-0 text-right text-xs text-(--muted)">{{ megabytes(m.size) }}</span>
            <span class="w-[70px] shrink-0"><KindBadge :kind="m.managed ? 'pack' : 'own'">{{ m.managed ? "Pack" : "Own" }}</KindBadge></span>
            <span class="flex w-[14rem] shrink-0 items-center justify-end gap-1.5">
              <AppButton
                v-if="modPage(m, known, instance.loader)"
                variant="secondary"
                size="small"
                icon="external"
                :aria-label="`Open the page of ${m.name} on ${modSource(instance.loader)}`"
                :title="`Open its page on ${modSource(instance.loader)}`"
                @click="run(() => openPage(modPage(m, known, instance!.loader)!))"
              />
              <AppButton
                v-if="updatable(m)"
                variant="secondary"
                size="small"
                :disabled="busy"
                :aria-label="`Update ${updatable(m)?.title ?? m.name}`"
                @click="installMod(updatable(m)!.project!)"
              >Update</AppButton>
              <span v-if="m.managed" class="inline-flex items-center gap-1 text-xs text-(--muted)"><Icon name="lock" :size="12" />Managed</span>
              <AppButton v-else variant="danger" size="small" :aria-label="`Remove ${m.name}`" @click="removing = m">Remove</AppButton>
            </span>
          </div>
          <div v-if="shown.length === 0" class="row justify-center text-(--muted)">{{ noModsText(query, kind) }}</div>
        </template>
      </section>

      <section v-if="!isHorizon(instance.loader)" class="card" aria-labelledby="java-heading">
        <div class="card-head">
          <h2 id="java-heading" class="card-title mc-name">Java arguments</h2>
          <span class="card-meta">Extra settings for the game’s Java. Leave this empty unless you know what they do.</span>
        </div>
        <div class="card-body flex flex-col gap-3">
          <div v-if="instance.server_java_args.length" class="text-xs text-(--muted)">
            <span class="inline-flex items-center gap-1"><Icon name="lock" :size="12" />Set by the server, on every sync:</span>
            <span class="mono ml-1 break-all">{{ instance.server_java_args.join(" ") }}</span>
          </div>
          <Note v-if="instance.refused_java_args.length" kind="warn">
            Portal left out what a server may not set on your PC: <span class="mono break-all">{{ instance.refused_java_args.join(" ") }}</span>
          </Note>
          <form class="flex flex-wrap items-end gap-3" @submit.prevent="saveJavaArgs">
            <div class="min-w-0 flex-1">
              <label class="label" for="java-args">Your own</label>
              <input id="java-args" v-model="javaArgs" class="input mono w-full" maxlength="2000" spellcheck="false" placeholder="-XX:+UseZGC" />
            </div>
            <AppButton variant="secondary" type="submit" :disabled="javaArgs.trim() === instance.java_args">Save</AppButton>
          </form>
          <p class="m-0 text-xs text-(--muted)">
            Used from the next start, after the memory above and the server’s, so yours win. Put quotes around a value with a space. If the game no longer starts, the Log
            says which one Java didn’t accept.
          </p>
        </div>
      </section>
    </template>
  </div>

  <ConfirmDialog :open="removing !== null" :title="`Remove this ${words.one}?`" :action="`Remove ${words.one}`" @confirm="confirmRemove" @cancel="removing = null">
    This deletes <span class="mono font-bold">{{ removing?.name }}</span> from {{ instance?.name }}’s {{ words.many }} folder. It’s your own file, so no sync will bring it back.
  </ConfirmDialog>
  <ConfirmDialog :open="deleting" :title="`Delete ${instance?.name}?`" action="Delete instance" @confirm="confirmDelete" @cancel="deleting = false">
    {{ instance ? deleteBody(instance, kind === "mod" ? mods : null) : "" }}
  </ConfirmDialog>
</template>
