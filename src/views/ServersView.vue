<script lang="ts">
import { ref } from "vue";
import type { CommandError, PanelServers } from "../api";

// Outside the component, so leaving and re-entering the view shows the earlier answer at once.
const list = ref<PanelServers[] | null>(null);
const loading = ref(false);
const loadError = ref<CommandError | null>(null);
</script>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import { join, servers, type Panel, type Server } from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import Card from "../components/Card.vue";
import EmptyState from "../components/EmptyState.vue";
import Icon from "../components/Icon.vue";
import Note from "../components/Note.vue";
import Skeleton from "../components/Skeleton.vue";
import StatePill from "../components/StatePill.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { andList, errorText, minecraftText } from "../logic/format";
import { look } from "../logic/look";
import { afterJoin, markPlaying, op, openInstance, playing, view, withProgress } from "../logic/operation";
import { joinButton, joinNote, playersText, runsText, serverKey, serversState, serversSummary } from "../logic/servers";
import { serverPill, serverTexture } from "../logic/visuals";

const state = computed(() => serversState(list.value));
const summary = computed(() => serversSummary(list.value, loadError.value !== null));
const opKey = computed(() => (op.value.phase === "idle" ? null : (op.value.key ?? null)));
const blocky = computed(() => look.value.theme === "blocky");
const panelNames = computed(() => andList((list.value ?? []).map((p) => p.panel.name)));
const skeletonRow: Array<[string, string]> = [["32px", "32px"], ["flex", "14px"], ["90px", "14px"], ["70px", "30px"]];

async function load() {
  if (loading.value) return;
  loading.value = true;
  try {
    list.value = await servers();
    loadError.value = null;
  } catch (e) {
    loadError.value = e as CommandError;
  } finally {
    loading.value = false;
  }
}

const button = (panel: Panel, s: Server) => joinButton(s, serverKey(panel.id, s.slug), op.value, opKey.value, playing.value);

async function doJoin(panel: Panel, s: Server) {
  const done = await withProgress(s.name, () => join(panel.id, s.slug), serverKey(panel.id, s.slug));
  if (!done) return;
  const { instance, joined } = done;
  markPlaying(instance, s.hostname);
  const text = joinNote(joined);
  afterJoin.value = text ? { instance: instance.id, text } : null;
  if (instance.manual.length > 0 || text) openInstance(instance.id);
}

onMounted(load);
</script>

<template>
  <ViewHead title="Servers" :summary="summary">
    <template #actions>
      <AppButton variant="secondary" icon="refresh" :disabled="loading" @click="load">Refresh</AppButton>
    </template>
  </ViewHead>
  <div class="content">
    <Note v-if="loadError && list" kind="err" role="alert">
      <strong>Couldn’t ask your panels.</strong>
      <div>{{ errorText(loadError) }}</div>
    </Note>

    <Note v-if="loadError && !list" kind="err" role="alert">
      <strong>Couldn’t ask your panels.</strong>
      <div>{{ errorText(loadError) }}</div>
      <AppButton variant="secondary" size="small" class="mt-2" :disabled="loading" @click="load">Try again</AppButton>
    </Note>

    <Card v-else-if="state === 'loading'" aria-busy="true">
      <div class="card-head"><span class="block h-3.5 w-40 animate-pulse rounded-xs bg-(--divider) motion-reduce:animate-none" aria-hidden="true" /></div>
      <div class="px-4"><Skeleton :rows="[skeletonRow, skeletonRow, skeletonRow]" /></div>
    </Card>

    <EmptyState v-else-if="state === 'none-linked'" title="No panels linked yet">
      Link a Spawnpoint panel and its servers appear here, ready to join with one click.
      <template #action>
        <AppButton variant="primary" icon="link" @click="view = 'panels'">Link a panel</AppButton>
      </template>
    </EmptyState>

    <Note v-else-if="state === 'none-answered'" kind="err" role="alert">
      <strong>No panel answered.</strong>
      <div>{{ panelNames }} cannot be reached. Check your internet connection, then try again.</div>
      <AppButton variant="secondary" size="small" class="mt-2" :disabled="loading" @click="load">Try again</AppButton>
    </Note>

    <Card v-else>
      <template v-for="entry in list" :key="entry.panel.id">
        <div class="card-head">
          <h2 class="card-title mc-name truncate" :title="entry.panel.name">{{ entry.panel.name }}</h2>
          <span class="mono truncate text-(--muted)" style="min-width: 0" :title="entry.panel.address">{{ entry.panel.address }}</span>
          <span class="flex-1" />
          <span class="card-meta">{{ entry.error ? "not answering" : entry.servers.length === 1 ? "1 server" : `${entry.servers.length} servers` }}</span>
        </div>
        <div v-if="entry.error" class="row" role="alert">
          <Icon name="warning" :size="18" class="shrink-0 text-(--destructive)" />
          <span class="min-w-0 flex-1">
            <strong>{{ errorText(entry.error) }}</strong> <span class="text-(--muted)">Its servers are hidden until it answers.</span>
          </span>
          <AppButton variant="secondary" size="small" :disabled="loading" @click="load">Try again</AppButton>
        </div>
        <template v-else>
          <div v-for="s in entry.servers" :key="s.slug" class="row">
            <span v-if="blocky" class="flex w-[34px] shrink-0 items-center justify-center">
              <BlockCube :texture="serverTexture(s)" :size="26" :spin="s.state !== 'offline'" :bob="s.state === 'running'" :dim="s.state !== 'running'" />
            </span>
            <Tile v-else :name="s.name" :texture="serverTexture(s)" :error="s.state === 'error'" />
            <div class="min-w-0 flex-1">
              <div class="mc-name truncate text-[15px] font-bold" :title="s.name">{{ s.name }}</div>
              <div class="mono truncate text-xs text-(--muted)" :title="s.hostname">{{ s.hostname }}</div>
            </div>
            <div class="w-[150px] shrink-0">
              <StatePill :tone="serverPill(s.state).tone">{{ serverPill(s.state).text }}</StatePill>
              <div class="mt-[3px] text-xs text-(--muted)">{{ playersText(s.players) }}</div>
            </div>
            <div class="w-[170px] min-w-0 shrink-0">
              <div class="truncate font-semibold">{{ minecraftText(s.game_version) }}</div>
              <div class="truncate text-xs text-(--muted)" :title="runsText(s)">{{ runsText(s) }}</div>
            </div>
            <div class="flex w-32 shrink-0 flex-col items-end gap-[3px]">
              <AppButton
                variant="primary"
                class="min-w-24"
                :disabled="button(entry.panel, s).disabled"
                :aria-label="button(entry.panel, s).aria"
                @click="doJoin(entry.panel, s)"
                >{{ button(entry.panel, s).label }}</AppButton
              >
              <span class="text-right text-xs text-(--muted)">{{ button(entry.panel, s).hint }}</span>
            </div>
          </div>
        </template>
      </template>
    </Card>
  </div>
</template>
