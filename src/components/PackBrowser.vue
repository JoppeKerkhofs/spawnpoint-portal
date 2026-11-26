<script setup lang="ts">
import { onMounted, ref } from "vue";
import { packInstall, packVersions, packsSearch, type CommandError, type Instance, type ModHit, type PackVersion } from "../api";
import { errorText } from "../logic/format";
import { downloadsText, projectPage } from "../logic/mods";
import { busy, withProgress } from "../logic/operation";
import { defaultPackVersion, packVersionText } from "../logic/packs";
import { useSearch } from "../logic/search";
import AppButton from "./AppButton.vue";
import Icon from "./Icon.vue";
import Note from "./Note.vue";
import Picture from "./Picture.vue";
import Skeleton from "./Skeleton.vue";

// "done" also follows a failed install: it can leave an instance behind.
const emit = defineEmits<{ installed: [instance: Instance]; done: []; page: [url: string]; back: [] }>();

const { query, hits, more, loading, failure, load } = useSearch(packsSearch);
const field = ref<HTMLInputElement | null>(null);
const row: Array<[string, string]> = [["32px", "32px"], ["flex", "14px"], ["70px", "26px"]];

const chosen = ref<ModHit | null>(null);
const name = ref("");
const versions = ref<PackVersion[] | null>(null);
const version = ref("");
const versionsError = ref<CommandError | null>(null);

async function choose(h: ModHit) {
  chosen.value = h;
  name.value = h.title;
  versions.value = null;
  version.value = "";
  versionsError.value = null;
  try {
    const list = await packVersions(h.project_id, "");
    if (chosen.value !== h) return;
    versions.value = list;
    version.value = defaultPackVersion(list);
  } catch (e) {
    if (chosen.value === h) versionsError.value = e as CommandError;
  }
}

async function install() {
  const h = chosen.value;
  const v = versions.value?.find((x) => x.id === version.value);
  if (!h || !v || !name.value.trim()) return;
  const made = await withProgress(name.value.trim(), () => packInstall(name.value.trim(), h.project_id, v.id, v.name, h.icon), `pack:${h.project_id}`);
  emit("done");
  if (made) emit("installed", made);
}

onMounted(() => field.value?.focus());
</script>

<template>
  <section class="card" aria-labelledby="packs-heading">
    <div class="card-head flex-wrap">
      <h2 id="packs-heading" class="card-title mc-name">Install a pack</h2>
      <span class="card-meta">Modrinth</span>
      <span class="flex-1" />
      <label for="packs-search" class="sr-only">Search Modrinth packs</label>
      <input id="packs-search" ref="field" v-model="query" class="input" style="width: 220px; height: 30px" placeholder="Search Modrinth packs" @keydown.enter="load()" />
      <AppButton variant="secondary" size="small" @click="emit('back')">Close</AppButton>
    </div>
    <div v-if="loading && !hits.length" class="px-4" aria-busy="true">
      <Skeleton :rows="[row, row, row]" />
    </div>
    <div v-else-if="!hits.length && !failure" class="row justify-center text-(--muted)" role="status">No packs found.</div>
    <template v-else-if="hits.length">
      <template v-for="h in hits" :key="h.project_id">
        <div class="row" style="padding-block: 8px">
          <Picture :picture="h.icon" :size="32"><Icon name="box" :size="32" class="text-(--muted)" /></Picture>
          <span class="min-w-0 flex-1">
            <span class="flex items-baseline gap-2">
              <span class="truncate font-bold">{{ h.title }}</span>
              <span class="shrink-0 text-xs text-(--muted)">by {{ h.author }}</span>
            </span>
            <span class="block truncate text-xs text-(--muted)">{{ h.description }}</span>
          </span>
          <span class="tabular w-36 shrink-0 text-right text-xs text-(--muted)">{{ downloadsText(h.downloads) }}</span>
          <AppButton
            variant="secondary"
            size="small"
            icon="external"
            :aria-label="`Open the page of ${h.title} on Modrinth`"
            title="Open its page on Modrinth"
            @click="emit('page', projectPage(null, h.project_id))"
          />
          <span class="flex w-28 shrink-0 justify-end">
            <AppButton
              variant="secondary"
              size="small"
              :aria-expanded="chosen === h"
              :aria-label="`Choose a version of ${h.title}`"
              @click="chosen === h ? (chosen = null) : choose(h)"
            >
              Choose
            </AppButton>
          </span>
        </div>
        <form v-if="chosen === h" class="row flex-wrap items-end gap-3" :aria-label="`Install ${h.title}`" @submit.prevent="install">
          <div class="min-w-0 flex-[1_1_200px]">
            <label class="label" :for="`pack-name-${h.project_id}`">Instance name</label>
            <input :id="`pack-name-${h.project_id}`" v-model="name" class="input w-full" />
          </div>
          <div class="min-w-0 flex-[2_1_260px]">
            <label class="label" :for="`pack-version-${h.project_id}`">Pack version</label>
            <select :id="`pack-version-${h.project_id}`" v-model="version" class="select w-full" :disabled="!versions?.length">
              <option v-if="!versions?.length" value="" disabled>{{ versionsError ? "Unavailable" : versions ? "No version Portal can install" : "Loading…" }}</option>
              <option v-for="v in versions ?? []" :key="v.id" :value="v.id">{{ packVersionText(v) }}</option>
            </select>
          </div>
          <AppButton variant="primary" type="submit" :disabled="busy || !version || !name.trim()">Install</AppButton>
          <Note v-if="versionsError" kind="err" role="alert" class="basis-full">
            <strong>Couldn’t load the versions of {{ h.title }}.</strong>
            <div>{{ errorText(versionsError) }}</div>
            <AppButton variant="secondary" size="small" class="mt-2" @click="choose(h)">Try again</AppButton>
          </Note>
        </form>
      </template>
      <div v-if="more" class="row justify-center">
        <AppButton variant="secondary" size="small" :disabled="loading" @click="load(hits.length)">{{ loading ? "Loading…" : "Load more" }}</AppButton>
      </div>
    </template>
    <div v-if="failure" class="px-4 pb-4 pt-3" role="alert">
      <Note kind="err">
        <strong>Couldn’t reach Modrinth.</strong>
        <div>{{ errorText(failure) }}</div>
        <AppButton variant="secondary" size="small" class="mt-2" @click="load(hits.length)">Try again</AppButton>
      </Note>
    </div>
  </section>
</template>
