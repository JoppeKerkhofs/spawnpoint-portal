import { ref } from "vue";
import type { Instance, Listing, Loader, MinecraftVersion, PackRef, Panel } from "../api";
import type { Op, Playing } from "./operation";
import { catalogueName, loaderName } from "./servers";

// Shared with the One instance view, which drops a deleted instance before switching back.
export const instanceList = ref<Listing | null>(null);

export function dropInstance(id: string): void {
  if (instanceList.value) instanceList.value = { ...instanceList.value, instances: instanceList.value.instances.filter((i) => i.id !== id) };
}

export type InstancesState = "loading" | "empty" | "filled";

export function instancesState(l: Listing | null): InstancesState {
  if (!l) return "loading";
  return l.instances.length === 0 && l.unreadable.length === 0 ? "empty" : "filled";
}

export function instancesSummary(l: Listing | null): string {
  switch (instancesState(l)) {
    case "loading":
      return "Reading your instances…";
    case "empty":
      return "Nothing here yet";
  }
  const n = l!.instances.length;
  const k = l!.unreadable.length;
  return `${n} instance${n === 1 ? "" : "s"}${k > 0 ? ` · ${k} couldn’t be read` : ""}`;
}

export const loaderText = (l: Loader | null): string => (l ? `${loaderName(l.kind)} ${l.version ?? "(newest)"}` : "No mod loader");

export function kindOf(i: Instance): "Server" | "Pack" | "By hand" {
  return i.source.kind === "server" ? "Server" : i.source.kind === "pack" ? "Pack" : "By hand";
}

export const packText = (p: PackRef): string => `${catalogueName(p.catalogue)} pack${p.version_name ? ` ${p.version_name}` : ""}`;

export function sourceText(i: Instance, panels: Panel[]): string {
  const s = i.source;
  if (s.kind === "manual") return "Made by you";
  if (s.kind === "pack") return packText(s.pack);
  const on = `${i.name} on ${panels.find((p) => p.id === s.panel)?.name ?? s.panel}`;
  return s.pack ? `${on} · ${packText(s.pack)}` : on;
}

export const versionChoices = (versions: MinecraftVersion[] | null, snapshots: boolean): string[] | null =>
  versions && versions.filter((v) => snapshots || !v.snapshot).map((v) => v.id);

// What instance_create takes: nothing, "fabric" for the newest, or "fabric:0.16.9".
export const loaderChoice = (loader: string, version: string): string | null =>
  loader === "none" ? null : version ? `${loader}:${version}` : loader;

export const canCreate = (name: string, versions: string[] | null): boolean => name.trim() !== "" && versions !== null && versions.length > 0;

// Shared by the list rows and the instance page.
export function playButton(i: Instance, op: Op, opKey: string | null, playing: Playing[]): { label: string; aria: string; disabled: boolean } {
  const working = op.phase === "preparing" || op.phase === "starting";
  if (playing.some((p) => p.instance === i.id)) return { label: "Running", aria: `${i.name} is running`, disabled: true };
  const label = working && opKey === i.id ? (op.phase === "preparing" ? "Preparing…" : "Starting…") : "Play";
  return { label, aria: `${label} ${i.name}`, disabled: working };
}

// The board colours a server instance like an own mod (green), a pack grey, a hand-made one tan.
export const kindTone = (i: Instance): "own" | "pack" | "hand" => (i.source.kind === "server" ? "own" : i.source.kind === "pack" ? "pack" : "hand");
