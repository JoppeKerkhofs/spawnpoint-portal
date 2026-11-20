<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { settingsGet, settingsSet, type CommandError } from "../api";
import AppButton from "../components/AppButton.vue";
import Note from "../components/Note.vue";
import ViewHead from "../components/ViewHead.vue";
import { nameHelp } from "../logic/account";
import { errorText } from "../logic/format";
import { look, setLook, type Appearance, type Theme } from "../logic/look";
import { playerName } from "../logic/operation";

const name = ref(playerName.value);
const saved = ref(playerName.value);
const justSaved = ref(false);
const saving = ref(false);
const error = ref<CommandError | null>(null);

const help = computed(() => nameHelp(name.value, saved.value, justSaved.value));

onMounted(async () => {
  try {
    const s = await settingsGet();
    if (name.value === saved.value) name.value = s.player_name;
    saved.value = s.player_name;
    playerName.value = s.player_name;
  } catch (e) {
    error.value = e as CommandError;
  }
});

async function save() {
  if (!help.value.canSave || saving.value) return;
  saving.value = true;
  error.value = null;
  try {
    const s = await settingsSet(name.value);
    saved.value = s.player_name;
    name.value = s.player_name;
    playerName.value = s.player_name;
    justSaved.value = true;
  } catch (e) {
    error.value = e as CommandError;
  } finally {
    saving.value = false;
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
  <ViewHead title="Account" summary="Who you are when you join a server." />
  <div class="content">
    <div class="flex max-w-[760px] flex-col gap-3">
    <form class="card" aria-labelledby="name-title" @submit.prevent="save">
      <div class="card-head">
        <h2 id="name-title" class="card-title mc-name">Player name</h2>
        <span class="badge" data-kind="pack">Offline</span>
      </div>
      <div class="card-body flex flex-col gap-2.5">
        <div class="flex flex-wrap items-end gap-2.5">
          <div class="min-w-0 flex-[1_1_260px]">
            <label class="label" for="player-name">Name other players see</label>
            <input
              id="player-name"
              v-model="name"
              class="input w-full"
              maxlength="16"
              autocomplete="off"
              spellcheck="false"
              aria-describedby="player-name-help"
              :aria-invalid="help.invalid"
              @input="justSaved = false"
            />
          </div>
          <AppButton variant="primary" type="submit" :disabled="!help.canSave || saving">{{ justSaved && name === saved ? "Saved" : "Save" }}</AppButton>
        </div>
        <div id="player-name-help" class="text-xs" :class="help.invalid ? 'text-(--destructive)' : ''">{{ help.text }}</div>
        <Note v-if="error" kind="err" role="alert">{{ errorText(error) }}</Note>
      </div>
    </form>

    <Note kind="warn" role="note">
      <strong>Servers in online mode will refuse this name.</strong>
      Without a Microsoft sign-in, you can only join servers that allow offline players. If a server turns you away with “Failed to verify username”, ask its owner to turn online mode off.
    </Note>

    <section class="card" aria-labelledby="microsoft-title">
      <div class="card-head">
        <h2 id="microsoft-title" class="card-title mc-name">Microsoft account</h2>
        <span class="badge" data-kind="hand">Coming later</span>
      </div>
      <div class="card-body flex flex-wrap items-center gap-5">
        <div class="flex min-w-0 flex-[1_1_380px] flex-col gap-1.5">
          <p class="m-0">Sign in with the Microsoft account that owns Minecraft to join any server under your real name.</p>
          <p class="m-0 text-(--muted)">It will work like linking a panel: Portal shows a code in large type, you enter it on Microsoft’s page, and Portal notices when you’re done. One account at a time.</p>
        </div>
        <AppButton variant="secondary" disabled>Sign in with Microsoft</AppButton>
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
              <option value="system">Follow Windows</option>
            </select>
          </div>
        </div>
        <div class="text-xs text-(--muted)">Stored on this PC only.</div>
      </div>
    </section>
    </div>
  </div>
</template>
