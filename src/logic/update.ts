import { ref, type Ref } from "vue";
import { updateCheck, updateInstall } from "../api";
import { normalizeError, type CommandError } from "./errors";

export type Update =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "current" }
  | { phase: "available"; version: string }
  | { phase: "installing"; version: string }
  | { phase: "failed"; error: CommandError; version: string | null };

export const update: Ref<Update> = ref({ phase: "idle" });

export function availableVersion(u: Update): string | null {
  return "version" in u ? u.version : null;
}

// The same two messages the backend refuses with, so the card can say why before the click.
export function installBlock(busy: boolean, playing: boolean): string | null {
  return playing ? "Close your game before updating." : busy ? "Wait for the current task to finish." : null;
}

export function canInstall(u: Update, busy: boolean, playing: boolean): boolean {
  return (u.phase === "available" || (u.phase === "failed" && u.version !== null)) && !installBlock(busy, playing);
}

// manual=false is the check at start: it only ever reports a found version.
export async function checkForUpdate(manual: boolean): Promise<void> {
  if (manual ? update.value.phase === "checking" : update.value.phase !== "idle") return;
  update.value = { phase: "checking" };
  try {
    const version = await updateCheck();
    update.value = version ? { phase: "available", version } : { phase: "current" };
  } catch (e) {
    update.value = manual ? { phase: "failed", error: normalizeError(e), version: null } : { phase: "idle" };
  }
}

export async function installUpdate(): Promise<void> {
  const version = availableVersion(update.value);
  if (!version || update.value.phase === "installing") return;
  update.value = { phase: "installing", version };
  try {
    await updateInstall();
  } catch (e) {
    update.value = { phase: "failed", error: normalizeError(e), version };
  }
}
