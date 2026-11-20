<script setup lang="ts">
import { computed, onMounted } from "vue";
import type { Panel, Server } from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import Card from "../components/Card.vue";
import EmptyState from "../components/EmptyState.vue";
import Icon from "../components/Icon.vue";
import Note from "../components/Note.vue";
import Skeleton from "../components/Skeleton.vue";
import StatePill from "../components/StatePill.vue";
import Picture from "../components/Picture.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { andList, errorText, minecraftText } from "../logic/format";
import { look } from "../logic/look";
import { op, openServer, playing, view } from "../logic/operation";
import { joinButton, joinServer, list, loadError, loadServers as load, loading, playersText, runsText, serverKey, serversState, serversSummary } from "../logic/servers";
import { serverPill, serverTexture } from "../logic/visuals";

const state = computed(() => serversState(list.value));
const summary = computed(() => serversSummary(list.value, loadError.value !== null));
const opKey = computed(() => (op.value.phase === "idle" ? null : (op.value.key ?? null)));
const blocky = computed(() => look.value.theme === "blocky");
const panelNames = computed(() => andList((list.value ?? []).map((p) => p.panel.name)));
const skeletonRow: Array<[string, string]> = [["32px", "32px"], ["flex", "14px"], ["90px", "14px"], ["70px", "30px"]];

const button = (panel: Panel, s: Server) => joinButton(s, serverKey(panel.id, s.slug), op.value, opKey.value, playing.value);

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
          <div v-for="s in entry.servers" :key="s.slug" class="row row-link" @click="openServer(entry.panel.id, s.slug)">
            <Picture :picture="s.picture" :size="blocky ? 34 : 32">
              <span v-if="blocky" class="flex w-[34px] shrink-0 items-center justify-center">
                <BlockCube :texture="serverTexture(s)" :size="26" :spin="s.state !== 'offline'" :bob="s.state === 'running'" :dim="s.state !== 'running'" />
              </span>
              <Tile v-else :name="s.name" :texture="serverTexture(s)" :error="s.state === 'error'" />
            </Picture>
            <div class="min-w-0 flex-1">
              <button type="button" class="mc-name block max-w-full truncate text-left text-[15px] font-bold" :title="s.name" @click="openServer(entry.panel.id, s.slug)">{{ s.name }}</button>
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
            <div class="flex w-32 shrink-0 flex-col items-end gap-[3px]" @click.stop>
              <AppButton
                variant="primary"
                class="min-w-24"
                :disabled="button(entry.panel, s).disabled"
                :aria-label="button(entry.panel, s).aria"
                @click="joinServer(entry.panel, s)"
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
