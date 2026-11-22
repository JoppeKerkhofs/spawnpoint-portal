<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  defaults,
  instanceAddMod,
  instanceCheckManual,
  instanceDelete,
  instanceGet,
  instanceLaunch,
  instanceMods,
  instanceRemoveMod,
  instanceSetMemory,
  instanceSetPicture,
  modsInstall,
  modsInstalled,
  openModsFolder,
  openPage,
  panels,
  type CommandError,
  type Defaults,
  type Instance,
  type InstalledMod,
  type ModFile,
  type Panel,
} from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import Icon from "../components/Icon.vue";
import KindBadge from "../components/KindBadge.vue";
import ModBrowser from "../components/ModBrowser.vue";
import Note from "../components/Note.vue";
import SegFilter from "../components/SegFilter.vue";
import Skeleton from "../components/Skeleton.vue";
import Picture from "../components/Picture.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { errorText, lastPlayedText, megabytes, memoryLabel } from "../logic/format";
import { dropInstance, playButton } from "../logic/instances";
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
  memoryOptions,
  modCounts,
  modsText,
  noModsText,
  orderMods,
  skippedName,
  modIcon,
  modPage,
  updatableMod,
  type ModFilter,
} from "../logic/mods";
import { instanceRowKey, leave } from "../logic/focus";
import { afterJoin, busy, lastPlayed, markPlaying, op, openInstanceId, playing, withProgress } from "../logic/operation";
import { instanceTexture } from "../logic/visuals";

const id = openInstanceId.value ?? "";
const instance = ref<Instance | null>(null);
const mods = ref<ModFile[] | null>(null);
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

async function loadMods() {
  try {
    mods.value = await instanceMods(id);
    modsError.value = null;
  } catch (e) {
    modsError.value = e as CommandError;
  }
}

// An extra after the list: without Modrinth the list is the same, only without its buttons.
async function loadKnown() {
  try {
    known.value = await modsInstalled(id);
  } catch {
    known.value = null;
  }
}

const refresh = async () => {
  await loadMods();
  await loadKnown();
};

async function installMod(project: string) {
  actionError.value = null;
  modsNote.value = null;
  const done = await withProgress(instance.value!.name, () => modsInstall(id, project), `mod:${project}`);
  if (done) modsNote.value = { text: installedSentence(done), kind: done.failed.length ? "warn" : "info" };
  await refresh();
}

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
    if (await instanceAddMod(id)) await refresh();
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
    await instanceRemoveMod(id, mod.name);
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
watch(instance, (i) => (lastPlayed.value = i ? lastPlayedText(i.name, i.last_played, new Date()) : null));

onMounted(() => {
  loadInstance();
  loadMods().then(loadKnown);
  panels().then((p) => (knownPanels.value = p), () => {});
  defaults().then((d) => (choices.value = d), () => {});
});
onUnmounted(() => {
  lastPlayed.value = null;
  afterJoin.value = null;
});
</script>

<template>
  <ViewHead :title="instance?.name ?? 'Instance'" :summary="instance ? instanceSummary(instance, knownPanels) : ''">
    <template #back>
      <button type="button" class="inline-flex items-center gap-1 text-xs hover:underline" @click="leave(instanceRowKey(id), 'instances')">
        <Icon name="chevron-left" :size="14" />Instances
      </button>
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
      <div class="flex items-center gap-2">
        <label for="instance-memory" class="text-[15px] text-(--muted)">Memory</label>
        <select id="instance-memory" class="select w-[92px]" :value="memory ?? ''" :disabled="memory === null" @change="setMemory">
          <option v-for="mb in memoryChoices" :key="mb" :value="mb">{{ memoryLabel(mb) }}</option>
        </select>
      </div>
      <AppButton v-if="instance.own_picture" variant="secondary" @click="removePicture">Remove picture</AppButton>
      <AppButton variant="secondary" icon="folder" @click="run(() => openModsFolder(id))">Mods folder</AppButton>
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
            <AppButton variant="secondary" size="small" icon="folder" @click="run(() => openModsFolder(id))">Open mods folder</AppButton>
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

      <ModBrowser
        v-if="browsing && instance.loader"
        :id="id"
        :minecraft="instance.minecraft"
        :loader="instance.loader"
        :installed="known"
        @install="installMod"
        @page="(url) => run(() => openPage(url))"
        @back="closeBrowser"
      />
      <section v-else class="card" aria-labelledby="mods-heading">
        <div class="card-head flex-wrap">
          <h2 id="mods-heading" class="card-title mc-name">Mods</h2>
          <span class="card-meta tabular">{{ modsText(counts.all) }}</span>
          <span class="flex-1" />
          <SegFilter v-model="filter" :options="filters" />
          <label for="mod-search" class="sr-only">Search mods</label>
          <input id="mod-search" v-model="query" class="input" style="width: 170px; height: 30px" placeholder="Search mods" />
          <AppButton v-if="instance.loader" ref="browseButton" variant="secondary" size="small" icon="plus" @click="browsing = true">Browse mods</AppButton>
          <AppButton variant="secondary" size="small" icon="upload" @click="addMod">Add a mod file</AppButton>
        </div>
        <div v-if="browseNote(instance)" class="row text-xs text-(--muted)">{{ browseNote(instance) }}</div>
        <div class="row gap-2 text-xs">
          <Icon name="lock" :size="14" class="shrink-0" />
          <span>Pack mods are managed by the sync and can change with the next pack update. Your own mods are never changed or removed by a sync.</span>
        </div>
        <div v-if="modsError && !mods" class="px-4 pb-4" role="alert">
          <Note kind="err">
            <strong>Couldn’t read the mods.</strong>
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
                v-if="modPage(m, known)"
                variant="secondary"
                size="small"
                icon="external"
                :aria-label="`Open the page of ${m.name} on Modrinth`"
                title="Open its page on Modrinth"
                @click="run(() => openPage(modPage(m, known)!))"
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
          <div v-if="shown.length === 0" class="row justify-center text-(--muted)">{{ noModsText(query) }}</div>
        </template>
      </section>
    </template>
  </div>

  <ConfirmDialog :open="removing !== null" title="Remove this mod?" action="Remove mod" @confirm="confirmRemove" @cancel="removing = null">
    This deletes <span class="mono font-bold">{{ removing?.name }}</span> from {{ instance?.name }}’s mods folder. It’s your own file, so no sync will bring it back.
  </ConfirmDialog>
  <ConfirmDialog :open="deleting" :title="`Delete ${instance?.name}?`" action="Delete instance" @confirm="confirmDelete" @cancel="deleting = false">
    {{ instance ? deleteBody(instance, mods) : "" }}
  </ConfirmDialog>
</template>
