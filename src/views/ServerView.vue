<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { instances } from "../api";
import AppButton from "../components/AppButton.vue";
import BlockCube from "../components/BlockCube.vue";
import Card from "../components/Card.vue";
import Icon from "../components/Icon.vue";
import Note from "../components/Note.vue";
import Picture from "../components/Picture.vue";
import StatePill from "../components/StatePill.vue";
import Tile from "../components/Tile.vue";
import ViewHead from "../components/ViewHead.vue";
import { errorText } from "../logic/format";
import { headUrl } from "../logic/picture";
import { look } from "../logic/look";
import { leave, serverRowKey } from "../logic/focus";
import { op, openInstance, openServerKey, playing } from "../logic/operation";
import {
  detailRows,
  findServer,
  instanceFor,
  joinButton,
  joinServer,
  list,
  loadError,
  loadServers,
  loading,
  playersLine,
  serverKey,
  serverSummaryLine,
} from "../logic/servers";
import { serverPill, serverTexture } from "../logic/visuals";

const back = () => leave(serverRowKey(openServerKey.value?.panel ?? "", openServerKey.value?.slug ?? ""), "servers");
const found = computed(() => findServer(list.value, openServerKey.value?.panel ?? "", openServerKey.value?.slug ?? ""));
const instanceId = ref<string | null>(null);
const blocky = computed(() => look.value.theme === "blocky");
const opKey = computed(() => (op.value.phase === "idle" ? null : (op.value.key ?? null)));
const join = computed(() => found.value && joinButton(found.value.server, serverKey(found.value.panel.id, found.value.server.slug), op.value, opKey.value, playing.value));

async function findInstance() {
  const k = openServerKey.value;
  try {
    instanceId.value = k ? instanceFor((await instances()).instances, k.panel, k.slug) : null;
  } catch {
    instanceId.value = null;
  }
}

async function doJoin() {
  if (found.value && (await joinServer(found.value.panel, found.value.server))) await findInstance();
}

onMounted(() => {
  if (!list.value) loadServers();
  findInstance();
});
</script>

<template>
  <ViewHead :title="found?.server.name ?? 'Server'" :summary="found ? serverSummaryLine(found.server, found.panel) : ''">
    <template #back>
      <button type="button" class="inline-flex items-center gap-1 text-xs hover:underline" @click="back">
        <Icon name="chevron-left" :size="14" />Servers
      </button>
    </template>
    <template v-if="found" #lead>
      <Picture :picture="found.server.picture" :size="blocky ? 52 : 44">
        <span v-if="blocky" class="flex h-[52px] w-[52px] items-center justify-center">
          <BlockCube :texture="serverTexture(found.server)" :size="40" :spin="found.server.state !== 'offline'" :bob="found.server.state === 'running'" />
        </span>
        <Tile v-else :name="found.server.name" :texture="serverTexture(found.server)" :error="found.server.state === 'error'" :size="44" />
      </Picture>
    </template>
    <template #actions>
      <AppButton variant="secondary" icon="refresh" :disabled="loading" :aria-busy="loading" @click="loadServers">Refresh</AppButton>
      <AppButton v-if="instanceId" variant="secondary" @click="openInstance(instanceId)">Open instance</AppButton>
      <AppButton v-if="found && join" variant="primary" :disabled="join.disabled" :aria-label="join.aria" @click="doJoin">{{ join.label }}</AppButton>
    </template>
  </ViewHead>
  <div class="content">
    <Note v-if="loadError" kind="err" role="alert">
      <strong>Couldn’t ask your panels.</strong>
      <div>{{ errorText(loadError) }}</div>
    </Note>

    <Note v-if="!found && list && !loading" kind="info" role="status">
      <div>This server is no longer on the panel.</div>
      <AppButton variant="secondary" size="small" class="mt-2" @click="back">Back to Servers</AppButton>
    </Note>

    <template v-if="found">
      <p v-if="join?.hint" class="m-0 text-xs text-(--muted)">{{ join.hint }}</p>

      <Card>
        <section aria-labelledby="status-heading">
          <div class="card-head"><h2 id="status-heading" class="card-title mc-name">Status</h2></div>
          <div class="card-body">
            <StatePill :tone="serverPill(found.server.state).tone">{{ serverPill(found.server.state).text }}</StatePill>
            <p v-if="found.server.detail" class="mb-0 mt-2">{{ found.server.detail }}</p>
          </div>
        </section>
      </Card>

      <Card>
        <section aria-labelledby="players-heading">
          <div class="card-head">
            <h2 id="players-heading" class="card-title mc-name">Players</h2>
            <span v-if="found.server.players" class="card-meta tabular">{{ found.server.players.online }} of {{ found.server.players.max }} online</span>
          </div>
          <p v-if="playersLine(found.server)" class="card-body m-0">{{ playersLine(found.server) }}</p>
          <template v-else>
            <ul class="m-0 list-none p-0">
              <li v-for="(name, n) in found.server.players!.names" :key="n" class="row">
                <Picture :picture="headUrl(name)" :size="24"><Tile :name="name" :texture="serverTexture(found.server)" :size="24" /></Picture>
                <span class="mono min-w-0 break-all">{{ name }}</span>
              </li>
            </ul>
            <p class="m-0 border-t border-(--divider) px-4 py-2 text-xs text-(--muted)">Player heads are loaded from mc-heads.net.</p>
          </template>
        </section>
      </Card>

      <Card>
        <section aria-labelledby="details-heading">
          <div class="card-head"><h2 id="details-heading" class="card-title mc-name">Details</h2></div>
          <dl class="m-0">
            <div v-for="[label, value] in detailRows(found.server, found.panel)" :key="label" class="row">
              <dt class="w-40 shrink-0 text-(--muted)">{{ label }}</dt>
              <dd class="m-0 min-w-0 break-all">{{ value }}</dd>
            </div>
          </dl>
        </section>
      </Card>
    </template>
  </div>
</template>
