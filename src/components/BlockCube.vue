<script setup lang="ts">
import { computed } from "vue";
import type { Texture } from "../logic/visuals";

const props = defineProps<{ texture: Texture; size: number; spin?: boolean; bob?: boolean; dim?: boolean }>();

// top, front, the three other sides, bottom
const textures: Record<Texture, [string, string, string, string]> = {
  grass: ["grass-top", "grass-side", "grass-side", "dirt"],
  wood: ["wood-top", "wood-side", "wood-side", "wood-top"],
  sand: ["sand", "sand", "sand", "sand"],
  machine: ["metal", "core", "metal", "metal"],
  stone: ["stone", "stone", "stone", "stone"],
  wool: ["wool", "wool", "wool", "wool"],
  chest: ["chest-top", "chest-side", "chest-side", "chest-top"],
};
const faces = computed(() => textures[props.texture].map((t) => `mc-t-${t}`));
</script>

<template>
  <span
    class="mc-cube-wrap"
    :class="{ 'mc-bob': bob }"
    aria-hidden="true"
    :style="{ '--s': `${size}px`, filter: dim ? 'grayscale(0.75) brightness(0.85)' : undefined }"
  >
    <span class="mc-cube" :class="{ 'mc-spin': spin }">
      <span class="mc-face mc-f-top" :class="faces[0]" />
      <span class="mc-face mc-f-front" :class="faces[1]" />
      <span class="mc-face mc-f-right" :class="faces[2]" />
      <span class="mc-face mc-f-back" :class="faces[2]" />
      <span class="mc-face mc-f-left" :class="faces[2]" />
      <span class="mc-face mc-f-bottom" :class="faces[3]" />
    </span>
  </span>
</template>
