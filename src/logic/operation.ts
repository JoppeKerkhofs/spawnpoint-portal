import { computed, ref, type ComputedRef, type Ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { defaults, instanceGet, onProgress as listenProgress, running, type CommandError, type Progress } from "../api";
import { normalizeError } from "./errors";
import { clock, errorText, stageText } from "./format";

export type View = "servers" | "instances" | "instance" | "panels" | "account";
export const view: Ref<View> = ref("servers");
export const openInstanceId: Ref<string | null> = ref(null);
export const playerName: Ref<string> = ref("");
// Set by the One instance view for the status bar's idle text; null elsewhere.
export const lastPlayed: Ref<string | null> = ref(null);

export function openInstance(id: string): void {
  openInstanceId.value = id;
  view.value = "instance";
}

export type Op =
  | { phase: "idle" }
  | { phase: "preparing"; name: string; step: number; steps: number; stage: string; done: number; total: number }
  | { phase: "starting"; name: string }
  | { phase: "failed"; name: string; error: CommandError };

export interface Playing {
  instance: string;
  name: string;
  since: Date;
  host: string | null;
  memoryMb: number;
  serverKey: string | null;
}

export const op: Ref<Op> = ref({ phase: "idle" });
export const playing: Ref<Playing[]> = ref([]);
export const busy: ComputedRef<boolean> = computed(() => op.value.phase === "preparing" || op.value.phase === "starting");

export function onProgress(current: Op, p: Progress): Op {
  if (current.phase !== "preparing" && current.phase !== "starting") return current;
  if (p.stage === "starting") return { phase: "starting", name: current.name };
  return { phase: "preparing", name: current.name, step: p.step, steps: p.steps, stage: p.stage, done: p.done, total: p.total };
}

export async function withProgress<T>(name: string, run: () => Promise<T>): Promise<T | undefined> {
  if (busy.value) return undefined;
  op.value = { phase: "preparing", name, step: 0, steps: 0, stage: "", done: 0, total: 0 };
  let live = true;
  let unlisten: UnlistenFn | undefined;
  try {
    unlisten = await listenProgress((p) => {
      if (live) op.value = onProgress(op.value, p);
    });
    const result = await run();
    op.value = { phase: "idle" };
    return result;
  } catch (e) {
    op.value = { phase: "failed", name, error: normalizeError(e) };
    return undefined;
  } finally {
    live = false;
    unlisten?.();
  }
}

export function closeFailure(): void {
  op.value = { phase: "idle" };
}

export function gameExited(instance: string): void {
  playing.value = playing.value.filter((p) => p.instance !== instance);
}

// Rebuilds "Playing" after a reload; the host is not known then, so only "since" shows.
export async function loadPlaying(): Promise<void> {
  try {
    const [ids, d] = await Promise.all([running(), defaults()]);
    const found = await Promise.all(ids.map((id) => instanceGet(id)));
    playing.value = found.map((i) => ({
      instance: i.id,
      name: i.name,
      since: i.last_played ? new Date(i.last_played.started_at * 1000) : new Date(),
      host: null,
      memoryMb: i.memory_mb ?? d.default_memory_mb,
      serverKey: i.source.kind === "server" ? `${i.source.panel}/${i.source.server}` : null,
    }));
  } catch {
    // The status bar then just reads "Ready".
  }
}

export function trailing(
  v: View,
  current: Op,
  list: Playing[],
  ctx: { playerName: string; lastPlayed: string | null },
): { lead: string; muted: string } {
  switch (current.phase) {
    case "preparing":
      return {
        lead: `Preparing ${current.name}`,
        muted: current.step > 0 ? `Step ${current.step} of ${current.steps}: ${stageText(current.stage)}` : "",
      };
    case "starting":
      return { lead: `Starting Minecraft for ${current.name}…`, muted: "The game window opens in a moment. You can keep using Portal." };
    case "failed":
      return { lead: `Couldn’t prepare ${current.name}.`, muted: errorText(current.error) };
  }
  const p = list[list.length - 1];
  if (!p) return { lead: "Ready", muted: (v === "instance" && ctx.lastPlayed) || "Join a server to start playing." };
  const since = `since ${clock(p.since)}`;
  const muted =
    v === "instance"
      ? `with ${Number((p.memoryMb / 1024).toFixed(1))} GB of memory`
      : v === "account"
        ? `${since} · as ${ctx.playerName}`
        : p.host
          ? `${since} · connected to ${p.host}`
          : since;
  return { lead: `Playing ${p.name}`, muted };
}

// What a screen reader hears: state changes and each new stage, never the counts.
export function liveText(current: Op, list: Playing[]): string {
  if (current.phase === "preparing") return current.step > 0 ? stageText(current.stage) : `Preparing ${current.name}`;
  if (current.phase === "starting") return `Starting Minecraft for ${current.name}`;
  const p = list[list.length - 1];
  return current.phase === "idle" && p ? `Playing ${p.name}` : "";
}
