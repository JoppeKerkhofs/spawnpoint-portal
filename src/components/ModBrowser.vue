<script setup lang="ts">
import { onMounted, ref } from "vue";
import { modsSearch, type InstalledMod, type Loader } from "../api";
import { errorText } from "../logic/format";
import { useSearch } from "../logic/search";
import { downloadsText, modSource, noResultsText, projectPage, resultControl } from "../logic/mods";
import { busy, op } from "../logic/operation";
import AppButton from "./AppButton.vue";
import Icon from "./Icon.vue";
import KindBadge from "./KindBadge.vue";
import Note from "./Note.vue";
import Picture from "./Picture.vue";
import Skeleton from "./Skeleton.vue";

const props = defineProps<{ id: string; minecraft: string; loader: Loader; installed: InstalledMod[] | null }>();
const source = modSource(props.loader);
const emit = defineEmits<{ install: [project: string]; page: [url: string]; back: [] }>();

const { query, hits, more, loading, failure, load } = useSearch((q, offset) => modsSearch(props.id, q, offset));
const field = ref<HTMLInputElement | null>(null);
const row: Array<[string, string]> = [["32px", "32px"], ["flex", "14px"], ["70px", "26px"]];

const working = (project: string) => op.value.phase !== "idle" && op.value.key === `mod:${project}`;

onMounted(() => field.value?.focus());
</script>

<template>
  <section class="card" aria-labelledby="browse-heading">
    <div class="card-head flex-wrap">
      <h2 id="browse-heading" class="card-title mc-name">Browse mods</h2>
      <span class="card-meta">{{ source }}</span>
      <span class="flex-1" />
      <label for="browse-search" class="sr-only">Search {{ source }}</label>
      <input
        id="browse-search"
        ref="field"
        v-model="query"
        class="input"
        style="width: 220px; height: 30px"
        :placeholder="`Search ${source}`"
        @keydown.enter="load()"
      />
      <AppButton variant="secondary" size="small" icon="chevron-left" @click="emit('back')">Back to installed mods</AppButton>
    </div>
    <div v-if="loading && !hits.length" class="px-4" aria-busy="true">
      <Skeleton :rows="[row, row, row]" />
    </div>
    <div v-else-if="!hits.length && !failure" class="row justify-center text-(--muted)" role="status">{{ noResultsText(loader, minecraft) }}</div>
    <template v-else-if="hits.length">
      <div v-for="h in hits" :key="h.project_id" class="row" style="padding-block: 8px">
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
          :aria-label="`Open the page of ${h.title} on ${source}`"
          :title="`Open its page on ${source}`"
          @click="emit('page', projectPage(loader, h.project_id))"
        />
        <span class="flex w-28 shrink-0 justify-end">
          <template v-for="control in [resultControl(h.project_id, installed)]" :key="control">
            <KindBadge v-if="control === 'pack'" kind="pack">From the pack</KindBadge>
            <KindBadge v-else-if="control === 'installed'" kind="own">Installed</KindBadge>
            <KindBadge v-else-if="control === 'disabled'" kind="hand">Disabled</KindBadge>
            <AppButton
              v-else
              variant="secondary"
              size="small"
              :disabled="busy"
              :aria-label="`${control === 'update' ? 'Update' : 'Install'} ${h.title}`"
              @click="emit('install', h.project_id)"
            >
              {{ working(h.project_id) ? (control === "update" ? "Updating…" : "Installing…") : control === "update" ? "Update" : "Install" }}
            </AppButton>
          </template>
        </span>
      </div>
      <div v-if="more" class="row justify-center">
        <AppButton variant="secondary" size="small" :disabled="loading" @click="load(hits.length)">{{ loading ? "Loading…" : "Load more" }}</AppButton>
      </div>
    </template>
    <div v-if="failure" class="px-4 pb-4 pt-3" role="alert">
      <Note kind="err">
        <strong>Couldn’t reach {{ source }}.</strong>
        <div>{{ errorText(failure) }}</div>
        <AppButton variant="secondary" size="small" class="mt-2" @click="load(hits.length)">Try again</AppButton>
      </Note>
    </div>
  </section>
</template>
