<script setup lang="ts">
import { onMounted, ref, useId, watch } from "vue";
import AppButton from "./AppButton.vue";

const props = defineProps<{ open: boolean; title: string; action: string }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();

const dialog = ref<HTMLDialogElement>();
const cancelButton = ref<InstanceType<typeof AppButton>>();
const titleId = useId();
const bodyId = useId();
let opener: Element | null = null;

function sync() {
  const d = dialog.value;
  if (!d) return;
  if (props.open && !d.open) {
    opener = document.activeElement;
    d.showModal();
    (cancelButton.value?.$el as HTMLElement | undefined)?.focus();
  } else if (!props.open && d.open) {
    d.close();
    if (opener instanceof HTMLElement) opener.focus();
  }
}

watch(() => props.open, sync, { flush: "post" });
onMounted(sync);
</script>

<template>
  <dialog
    ref="dialog"
    class="dialog"
    role="alertdialog"
    :aria-labelledby="titleId"
    :aria-describedby="bodyId"
    @cancel.prevent="emit('cancel')"
    @close="props.open && emit('cancel')"
  >
    <h2 :id="titleId" class="dialog-title mc-name">{{ title }}</h2>
    <div :id="bodyId" class="dialog-body"><slot /></div>
    <div class="dialog-actions">
      <AppButton ref="cancelButton" variant="secondary" @click="emit('cancel')">Cancel</AppButton>
      <AppButton variant="danger-fill" @click="emit('confirm')">{{ action }}</AppButton>
    </div>
  </dialog>
</template>
