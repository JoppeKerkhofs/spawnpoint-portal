<script setup lang="ts">
defineProps<{ options: Array<{ value: string; label: string }>; modelValue: string }>();
defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="seg" role="group" aria-label="Show">
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      :aria-pressed="o.value === modelValue"
      @click="$emit('update:modelValue', o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>
