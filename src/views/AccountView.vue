<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { accountSignOut, appVersion, settingsGet, settingsSet, type CommandError } from "../api";
import AppButton from "../components/AppButton.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import Note from "../components/Note.vue";
import Spinner from "../components/Spinner.vue";
import ViewHead from "../components/ViewHead.vue";
import SignInView from "./SignInView.vue";
import { errorText } from "../logic/format";
import { look, setLook, type Appearance, type Theme } from "../logic/look";
import { busy, playerName, playing } from "../logic/operation";
import { followSystem, thisComputer } from "../logic/os";
import { availableVersion, canInstall, checkForUpdate, installBlock, installUpdate, update } from "../logic/update";

const discord = ref(true);
const error = ref<CommandError | null>(null);

const account = ref<string | null>(null);
const signingIn = ref(false);
const askSignOut = ref(false);
const signOutError = ref<CommandError | null>(null);

const version = ref("");
const newVersion = computed(() => availableVersion(update.value));
const block = computed(() => installBlock(busy.value, playing.value.length > 0));

onMounted(async () => {
  appVersion().then((v) => (version.value = v), () => {});
  try {
    const s = await settingsGet();
    discord.value = s.discord;
    account.value = s.account_name;
    playerName.value = s.account_name ?? "";
  } catch (e) {
    error.value = e as CommandError;
  }
});

async function setDiscord(on: boolean) {
  error.value = null;
  try {
    discord.value = (await settingsSet(on)).discord;
  } catch (e) {
    error.value = e as CommandError;
  }
}

function signedIn(name: string) {
  account.value = name;
  playerName.value = name;
}

async function signOut() {
  askSignOut.value = false;
  signOutError.value = null;
  try {
    await accountSignOut();
    account.value = null;
    playerName.value = "";
    signingIn.value = false;
  } catch (e) {
    signOutError.value = e as CommandError;
  }
}

const theme = computed({
  get: () => look.value.theme,
  set: (t: Theme) => setLook({ ...look.value, theme: t }),
});
const appearance = computed({
  get: () => look.value.appearance,
  set: (a: Appearance) => setLook({ ...look.value, appearance: a }),
});
</script>

<template>
  <SignInView v-if="signingIn" @back="signingIn = false" @signed-in="signedIn" @sign-out="askSignOut = true" />
  <ViewHead v-if="!signingIn" title="Account" summary="Who you are when you join a server." />
  <div v-if="!signingIn" class="content">
    <div class="flex max-w-[760px] flex-col gap-3">
    <section class="card" aria-labelledby="microsoft-title">
      <div class="card-head">
        <h2 id="microsoft-title" class="card-title mc-name">Microsoft account</h2>
        <span v-if="account" class="badge" data-kind="own">Signed in</span>
      </div>
      <div class="card-body flex flex-col gap-2.5">
        <div v-if="account" class="flex flex-wrap items-center gap-5">
          <p class="m-0 min-w-0 flex-[1_1_380px]">You play as <strong class="mc-name">{{ account }}</strong> on every server.</p>
          <AppButton variant="danger" @click="askSignOut = true">Sign out</AppButton>
        </div>
        <div v-else class="flex flex-wrap items-center gap-5">
          <div class="flex min-w-0 flex-[1_1_380px] flex-col gap-1.5">
            <p class="m-0">Sign in with the Microsoft account that owns Minecraft. Starting a game needs it.</p>
            <p class="m-0 text-(--muted)">Portal shows a code, you enter it on Microsoft’s page, and Portal notices when you’re done. One account at a time.</p>
          </div>
          <AppButton variant="primary" @click="signingIn = true">Sign in with Microsoft</AppButton>
        </div>
        <Note v-if="signOutError" kind="err" role="alert">{{ errorText(signOutError) }}</Note>
      </div>
    </section>

    <section class="card" aria-labelledby="appearance-title">
      <div class="card-head"><h2 id="appearance-title" class="card-title mc-name">Appearance</h2></div>
      <div class="card-body flex flex-col gap-2.5">
        <div class="flex flex-wrap gap-3">
          <div class="min-w-0 flex-[1_1_200px]">
            <label class="label" for="look-theme">Theme</label>
            <select id="look-theme" v-model="theme" class="select w-full">
              <option value="default">Standard</option>
              <option value="blocky">Blocky</option>
            </select>
          </div>
          <div v-if="theme === 'default'" class="min-w-0 flex-[1_1_200px]">
            <label class="label" for="look-appearance">Light or dark</label>
            <select id="look-appearance" v-model="appearance" class="select w-full">
              <option value="light">Light</option>
              <option value="dark">Dark</option>
              <option value="system">{{ followSystem }}</option>
            </select>
          </div>
        </div>
        <div class="text-xs text-(--muted)">Stored on {{ thisComputer }} only.</div>
      </div>
    </section>

    <section class="card" aria-labelledby="discord-title">
      <div class="card-head"><h2 id="discord-title" class="card-title mc-name">Discord</h2></div>
      <div class="card-body flex flex-col gap-2.5">
        <label class="flex items-center gap-2">
          <input type="checkbox" class="check" :checked="discord" @change="setDiscord(($event.target as HTMLInputElement).checked)" />
          Show what I'm playing on Discord
        </label>
        <div class="text-xs text-(--muted)">Discord shows "In menu" while the launcher is open, and the instance's name and Minecraft version while a game runs. Never the server's address.</div>
        <Note v-if="error" kind="err" role="alert">{{ errorText(error) }}</Note>
      </div>
    </section>

    <section class="card" aria-labelledby="about-title">
      <div class="card-head"><h2 id="about-title" class="card-title mc-name">About</h2></div>
      <div class="card-body flex flex-col gap-2.5">
        <div class="flex flex-wrap items-center gap-2.5">
          <span class="min-w-0 flex-[1_1_200px]">Version {{ version }}</span>
          <AppButton variant="secondary" :disabled="update.phase === 'checking' || update.phase === 'installing'" @click="checkForUpdate(true)">
            <Spinner v-if="update.phase === 'checking'" />Check for updates
          </AppButton>
        </div>
        <div v-if="update.phase === 'current'" role="status">You have the newest version.</div>
        <template v-if="newVersion">
          <div role="status">Version {{ newVersion }} is available.</div>
          <div v-if="update.phase === 'installing'" class="flex items-center gap-2" role="status"><Spinner />Downloading the update…</div>
          <div v-else class="flex flex-wrap items-center gap-2.5">
            <AppButton variant="primary" :disabled="!canInstall(update, busy, playing.length > 0)" @click="installUpdate">Install and restart</AppButton>
            <span v-if="block" class="text-xs text-(--muted)">{{ block }}</span>
          </div>
        </template>
        <Note v-if="update.phase === 'failed'" kind="err" role="alert">{{ errorText(update.error) }}</Note>
      </div>
    </section>
    </div>
  </div>
  <ConfirmDialog :open="askSignOut" :title="`Sign out of ${account ?? ''}?`" action="Sign out" @cancel="askSignOut = false" @confirm="signOut">
    Portal forgets your Microsoft sign-in. Your instances and worlds stay. You cannot start a game until you sign in again.
  </ConfirmDialog>
</template>
