<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { accountSignInPoll, accountSignInStart, openPage, type CommandError, type DeviceCode } from "../api";
import AppButton from "../components/AppButton.vue";
import Icon from "../components/Icon.vue";
import Note from "../components/Note.vue";
import PairingCode from "../components/PairingCode.vue";
import Picture from "../components/Picture.vue";
import Spinner from "../components/Spinner.vue";
import ViewHead from "../components/ViewHead.vue";
import { countdown, errorText } from "../logic/format";
import { view } from "../logic/operation";
import { headUrl } from "../logic/picture";
import { initials } from "../logic/visuals";

const emit = defineEmits<{ back: []; signedIn: [name: string]; signOut: [] }>();

const code = ref<DeviceCode | null>(null);
const player = ref<string | null>(null);
const error = ref<CommandError | null>(null);
const left = ref("");
const copied = ref(false);
// Each attempt owns a number; an answer for an older one is ignored.
let attempt = 0;
let poller: ReturnType<typeof setTimeout> | undefined;
let ticker: ReturnType<typeof setInterval> | undefined;

function stop() {
  attempt++;
  clearTimeout(poller);
  clearInterval(ticker);
  code.value = null;
}

async function start() {
  stop();
  const mine = attempt;
  error.value = null;
  try {
    const c = await accountSignInStart();
    if (mine !== attempt) return;
    code.value = c;
    const expiresAt = Date.now() + c.expires_in * 1000;
    const tick = () => (left.value = countdown(expiresAt - Date.now()));
    tick();
    ticker = setInterval(tick, 1000);
    const poll = () => {
      poller = setTimeout(async () => {
        try {
          const name = await accountSignInPoll();
          if (name !== null) {
            stop();
            player.value = name;
            emit("signedIn", name);
          } else if (mine === attempt) poll();
        } catch (e) {
          if (mine !== attempt) return;
          stop();
          error.value = e as CommandError;
        }
      }, (c.interval + 1) * 1000);
    };
    poll();
  } catch (e) {
    if (mine === attempt) error.value = e as CommandError;
  }
}

async function copy() {
  if (!code.value) return;
  await navigator.clipboard.writeText(code.value.user_code).catch(() => {});
  copied.value = true;
  setTimeout(() => (copied.value = false), 1500);
}

const shortAddress = (url: string) => url.replace(/^https:\/\/(www\.)?/, "");

onMounted(start);
onUnmounted(stop);
</script>

<template>
  <ViewHead title="Sign in with Microsoft" summary="Use the Microsoft account that owns Minecraft: Java Edition. Starting a game needs it.">
    <template #back>
      <button type="button" class="inline-flex items-center gap-1 text-xs hover:underline" @click="emit('back')">
        <Icon name="chevron-left" :size="14" />Account
      </button>
    </template>
  </ViewHead>
  <div class="content">
    <div class="flex max-w-[860px] flex-col gap-3">
      <section v-if="player" class="card" aria-labelledby="done-title">
        <div class="card-head">
          <h2 id="done-title" class="card-title mc-name">Signed in</h2>
          <span class="badge" data-kind="own">Online</span>
        </div>
        <div class="card-body flex flex-wrap items-center gap-4" role="status">
          <Picture :picture="headUrl(player)" :size="64"><span class="avatar" aria-hidden="true" style="width: 64px; height: 64px; font-size: 22px">{{ initials(player) }}</span></Picture>
          <div class="min-w-0 flex-[1_1_260px]">
            <div class="mc-name text-[22px] font-bold">{{ player }}</div>
            <div class="text-(--muted)">Minecraft: Java Edition · Microsoft account</div>
            <div class="mt-1 text-xs font-bold text-(--ok-icon)">Servers now accept you under this name.</div>
          </div>
          <div class="flex gap-2">
            <AppButton variant="primary" icon="server" @click="view = 'servers'">Go to Servers</AppButton>
            <AppButton variant="danger" @click="emit('signOut')">Sign out</AppButton>
          </div>
        </div>
      </section>

      <section v-else-if="code" class="card" aria-labelledby="code-title">
        <div class="card-head"><h2 id="code-title" class="card-title mc-name">Enter this code on Microsoft’s page</h2></div>
        <div class="card-body flex flex-wrap items-center gap-7">
          <div class="flex min-w-0 flex-[1_1_320px] flex-col gap-2.5">
            <div class="text-(--muted)">Go to <strong class="mono">{{ shortAddress(code.verification_uri) }}</strong> and type:</div>
            <PairingCode :code="code.user_code" />
            <div class="flex items-center gap-2" aria-live="polite"><Spinner /><span>Waiting for you to sign in. Portal notices by itself.</span></div>
            <div class="tabular text-xs text-(--muted)">The code expires in {{ left }}.</div>
          </div>
          <div class="flex min-w-0 flex-[1_1_300px] flex-col gap-2.5">
            <ol class="m-0 flex list-decimal flex-col gap-1 pl-5">
              <li>Open Microsoft’s page in your browser.</li>
              <li>Enter the code, then sign in with your Microsoft account.</li>
              <li>Allow Spawnpoint Portal to use your Minecraft profile.</li>
            </ol>
            <div class="flex flex-wrap gap-2">
              <AppButton variant="primary" icon="external" @click="openPage(code.verification_uri)">Open {{ shortAddress(code.verification_uri) }}</AppButton>
              <AppButton variant="secondary" @click="copy">{{ copied ? "Copied" : "Copy code" }}</AppButton>
              <AppButton variant="secondary" @click="emit('back')">Cancel</AppButton>
            </div>
          </div>
        </div>
      </section>

      <Note v-else-if="error" kind="err" role="alert">
        <div class="flex flex-col gap-2">
          <div>{{ errorText(error) }}</div>
          <div><AppButton variant="primary" size="small" @click="start">Get a new code</AppButton></div>
        </div>
      </Note>

      <div v-else class="flex items-center gap-2" role="status"><Spinner />Asking Microsoft for a code…</div>

      <Note kind="info" icon="lock" role="note">
        Your password goes only to Microsoft, on Microsoft’s own page. Portal keeps a sign-in token in Windows Credential Manager and never shows it. You can sign out at any time.
      </Note>
    </div>
  </div>
</template>
