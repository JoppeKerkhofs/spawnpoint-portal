<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { shownPicture } from "../logic/picture";

const props = defineProps<{ picture: string | null | undefined; size: number }>();
const failed = ref(false);
watch(() => props.picture, () => (failed.value = false));
const src = computed(() => shownPicture(props.picture, failed.value));
</script>

<template>
  <img
    v-if="src"
    :src="src"
    alt=""
    class="picture"
    :style="{ width: `${size}px`, height: `${size}px` }"
    @error="failed = true"
  />
  <slot v-else />
</template>
