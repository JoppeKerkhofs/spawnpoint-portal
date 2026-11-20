import { ref, nextTick } from "vue";
import { view, type View } from "./operation";

const returnTo = ref<string | null>(null);

export const instanceRowKey = (id: string): string => `instance:${id}`;
export const serverRowKey = (panel: string, slug: string): string => `server:${panel}/${slug}`;

export function leave(row: string, to: View): void {
  returnTo.value = row;
  view.value = to;
}

// A list view takes the row it is returned to when it opens, so a stale one never moves focus later.
export function takeReturn(): string | null {
  const row = returnTo.value;
  returnTo.value = null;
  return row;
}

// The row's name button; the h1 keeps focus when the row is gone or the player has moved on.
export async function focusRow(row: string | null): Promise<void> {
  await nextTick();
  const now = document.activeElement;
  if (!row || (now && now !== document.body && now.tagName !== "H1")) return;
  document.querySelector<HTMLElement>(`[data-row="${CSS.escape(row)}"] .mc-name`)?.focus();
}
