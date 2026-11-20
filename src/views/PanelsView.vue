<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  openPairingPage, panelPairCancel, panelPairPoll, panelPairStart, panelRemove, panels, servers,
  type CommandError, type Linked, type Panel, type PairingView, type PanelServers,
} from "../api";
import AppButton from "../components/AppButton.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import Icon from "../components/Icon.vue";
import Logo from "../components/Logo.vue";
import Note from "../components/Note.vue";
import PairingCode from "../components/PairingCode.vue";
import Spinner from "../components/Spinner.vue";
import ViewHead from "../components/ViewHead.vue";
import { countdown, errorText } from "../logic/format";
import { canGetCode, linkedStatus, missingText, needsConsent } from "../logic/pairing";
import { pairingFlow } from "../logic/pairingFlow";
import { busy, view } from "../logic/operation";

type Phase = "form" | "code" | "linked" | "expired";

const list = ref<Panel[] | null>(null);
const answers = ref<PanelServers[] | null>(null);
const loadError = ref<CommandError | null>(null);

const address = ref("https://");
const consent = ref(false);
const phase = ref<Phase>("form");
const starting = ref(false);
const startError = ref<CommandError | null>(null);
const pairing = ref<PairingView | null>(null);
const now = ref(Date.now());
const unreachable = ref(false);
const openError = ref<CommandError | null>(null);
const linked = ref<Linked | null>(null);
const failure = ref<CommandError | null>(null);
const removing = ref<Panel | null>(null);
const removeError = ref<CommandError | null>(null);

const httpAddress = computed(() => needsConsent(address.value));
const canStart = computed(() => canGetCode(address.value, consent.value) && !starting.value && !busy.value);
const left = computed(() => (pairing.value ? countdown(Date.parse(pairing.value.expires_at) - now.value) : ""));
const status = (p: Panel) => linkedStatus(answers.value?.find((a) => a.panel.id === p.id));

const body = ref<HTMLElement>();

async function load() {
  try {
    list.value = await panels();
    loadError.value = null;
  } catch (e) {
    loadError.value = e as CommandError;
    return;
  }
  answers.value = null;
  servers().then((a) => (answers.value = a), () => {}); // A failing call leaves the status column empty.
}

const flow = pairingFlow(
  { start: panelPairStart, poll: panelPairPoll, cancel: panelPairCancel },
  {
    code(p) {
      pairing.value = p;
      now.value = Date.now();
      unreachable.value = false;
      openError.value = null;
      phase.value = "code";
    },
    tick: (t) => (now.value = t),
    unreachable: (v) => (unreachable.value = v),
    linked(l) {
      linked.value = l;
      phase.value = "linked";
      load();
    },
    expired(error) {
      failure.value = error;
      phase.value = "expired";
    },
  },
);

function cancel() {
  flow.cancel();
  failure.value = null;
  phase.value = "form";
}

async function start() {
  if (!canStart.value) return;
  starting.value = true;
  startError.value = null;
  try {
    await flow.start(address.value.trim(), httpAddress.value && consent.value);
  } catch (e) {
    startError.value = e as CommandError;
  } finally {
    starting.value = false;
  }
}

function another() {
  address.value = "https://";
  consent.value = false;
  startError.value = null;
  phase.value = "form";
}

async function openPage() {
  openError.value = null;
  try {
    await openPairingPage();
  } catch (e) {
    openError.value = e as CommandError;
  }
}

async function remove() {
  const p = removing.value;
  if (!p) return;
  removing.value = null;
  try {
    await panelRemove(p.id);
    removeError.value = null;
  } catch (e) {
    removeError.value = e as CommandError;
  }
  load();
}

watch(phase, async () => {
  await nextTick();
  body.value?.querySelector<HTMLElement>("input, button, a[href]")?.focus();
});

onMounted(load);
onUnmounted(() => flow.leave());
</script>

<template>
  <ViewHead title="Panels" summary="The Spawnpoint panels whose servers you can join. Portal keeps each panel’s key in Windows Credential Manager and never shows it." />
  <div class="content">
    <Note v-if="loadError" kind="err" role="alert">
      <strong>Couldn’t read your linked panels.</strong>
      <div>{{ errorText(loadError) }}</div>
    </Note>
    <Note v-if="removeError" kind="err" role="alert">
      <strong>Couldn’t remove the panel.</strong>
      <div>{{ errorText(removeError) }}</div>
    </Note>

    <section v-if="list && list.length > 0" class="card" aria-labelledby="linked-title">
      <div class="card-head">
        <h2 id="linked-title" class="card-title mc-name">Linked panels</h2>
        <span class="card-meta">{{ list.length === 1 ? "1 panel" : `${list.length} panels` }}</span>
      </div>
      <div v-for="p in list" :key="p.id" class="row">
        <span class="flex w-[34px] shrink-0 items-center"><Logo :size="30" /></span>
        <div class="min-w-0 flex-1">
          <div class="mc-name truncate text-[15px] font-bold" :title="p.name">{{ p.name }}</div>
          <div class="mono truncate text-xs text-(--muted)" :title="p.address">{{ p.address }}</div>
        </div>
        <div class="flex w-[300px] shrink-0 items-center gap-2">
          <template v-if="status(p)">
            <span v-if="status(p)!.ok" class="dot is-playing" aria-hidden="true" />
            <Icon v-else name="warning" :size="16" class="shrink-0 text-(--destructive)" />
            <span class="min-w-0 truncate" :title="status(p)!.text">{{ status(p)!.text }}</span>
          </template>
        </div>
        <AppButton variant="danger" size="small" :aria-label="`Remove ${p.name}`" @click="removing = p">Remove</AppButton>
      </div>
    </section>

    <section class="card" aria-labelledby="link-title">
      <div class="card-head"><h2 id="link-title" class="card-title mc-name">Link a panel</h2></div>
      <div ref="body" class="card-body">
        <form v-if="phase === 'form'" class="flex max-w-[560px] flex-col gap-2.5" @submit.prevent="start">
          <div>
            <label class="label" for="panel-address">Panel address</label>
            <input
              id="panel-address"
              v-model="address"
              class="input mono w-full"
              placeholder="https://panel.example.com"
              autocomplete="off"
              spellcheck="false"
              aria-describedby="panel-address-help"
              @input="httpAddress || (consent = false)"
            />
            <div id="panel-address-help" class="mt-1 text-xs text-(--muted)">The address you open your Spawnpoint panel at.</div>
          </div>
          <div v-if="httpAddress" class="note" data-kind="warn" style="padding: 10px 12px">
            <Icon name="warning" :size="18" />
            <label class="note-text flex cursor-pointer items-start gap-2">
              <input v-model="consent" type="checkbox" class="check" />
              <span>This panel has no https. <strong>Link it anyway:</strong> I understand Portal’s key will travel unencrypted.</span>
            </label>
          </div>
          <Note v-if="startError" kind="err" role="alert">
            <strong>Couldn’t start linking.</strong>
            <div>{{ errorText(startError) }}</div>
          </Note>
          <div><AppButton variant="primary" icon="link" type="submit" :disabled="!canStart">Get a code</AppButton></div>
        </form>

        <div v-else-if="phase === 'code' && pairing" class="flex flex-wrap gap-7">
          <div class="flex min-w-0 flex-[1_1_320px] flex-col gap-2.5">
            <div class="text-(--muted)">Approve this code in <strong class="mono">{{ pairing.address }}</strong></div>
            <PairingCode :code="pairing.code" />
            <div class="flex items-center gap-2" aria-live="polite">
              <Spinner />
              <span v-if="unreachable">The panel cannot be reached. Portal keeps trying.</span>
              <span v-else>Waiting for approval. Portal notices by itself.</span>
            </div>
            <div class="tabular text-xs text-(--muted)">The code expires in {{ left }}.</div>
          </div>
          <div class="flex min-w-0 flex-[1_1_280px] flex-col gap-3">
            <ol class="m-0 list-decimal pl-5">
              <li>Open the approval page and sign in to your panel.</li>
              <li>Check that the panel shows the same code.</li>
              <li>Approve it. This window updates on its own.</li>
            </ol>
            <Note v-if="openError" kind="err" role="alert">{{ errorText(openError) }}</Note>
            <div class="flex gap-2">
              <AppButton variant="primary" icon="external" @click="openPage">Open the approval page</AppButton>
              <AppButton variant="secondary" @click="cancel">Cancel</AppButton>
            </div>
          </div>
        </div>

        <div v-else-if="phase === 'linked' && linked" role="status" class="flex flex-wrap items-center gap-3">
          <Icon name="check" :size="24" class="shrink-0 text-(--ok-icon)" />
          <div class="min-w-0 flex-1">
            <div class="truncate text-base font-bold" :title="linked.panel.name">Linked: {{ linked.panel.name }}</div>
            <div class="text-(--muted)">Its servers now show in Servers.</div>
            <div v-if="linked.missing.length > 0" class="mt-1">{{ missingText(linked.missing) }}</div>
          </div>
          <AppButton variant="primary" icon="server" @click="view = 'servers'">Go to Servers</AppButton>
          <AppButton variant="secondary" :disabled="busy" @click="another">Link another</AppButton>
        </div>

        <div v-else-if="phase === 'expired'" role="alert" class="flex flex-wrap items-center gap-3">
          <Icon name="warning" :size="22" class="shrink-0 text-(--destructive)" />
          <div class="min-w-0 flex-1">
            <div class="font-bold">{{ failure ? errorText(failure) : "The pairing code expired or was already used." }}</div>
            <div class="text-(--muted)">Codes last a few minutes. Get a new one and approve it in your panel.</div>
            <div v-if="startError" class="mt-1 text-(--destructive)">{{ errorText(startError) }}</div>
          </div>
          <AppButton variant="primary" :disabled="starting || busy" @click="start">Get a new code</AppButton>
        </div>
      </div>
    </section>

    <ConfirmDialog :open="!!removing" :title="`Remove ${removing?.name ?? ''}?`" action="Remove panel" @cancel="removing = null" @confirm="remove">
      Its servers disappear from Servers and Portal forgets its key. Instances you made from its servers stay, and you can link it again later.
    </ConfirmDialog>
  </div>
</template>
