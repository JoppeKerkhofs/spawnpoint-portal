<script setup lang="ts">
import Icon, { type IconName } from "./Icon.vue";

withDefaults(
  defineProps<{
    variant: "primary" | "secondary" | "danger" | "danger-fill";
    size?: "default" | "small";
    icon?: IconName;
    type?: "button" | "submit";
    disabled?: boolean;
  }>(),
  { size: "default", type: "button", disabled: false },
);
</script>

<template>
  <button
    :type="type"
    :disabled="disabled"
    class="btn"
    :class="[`btn-${variant}`, { 'btn-sm': size === 'small', 'has-icon': icon }]"
  >
    <Icon v-if="icon" :name="icon" :size="size === 'small' ? 14 : 16" />
    <slot />
  </button>
</template>
