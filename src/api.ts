import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface CommandError {
  kind: string;
  message: string;
}

export interface LastPlayed {
  started_at: number;
  seconds: number;
}

export interface PackRef {
  catalogue: string;
  project: string;
  version: string;
  version_name: string | null;
}

export type Source =
  | { kind: "manual" }
  | { kind: "pack"; pack: PackRef }
  | { kind: "server"; panel: string; server: string; pack: PackRef | null };

export interface Manual {
  path: string;
  name: string;
  page: string;
}

export interface Loader {
  kind: "fabric" | "neoforge" | "forge";
  version: string | null;
}

export interface ManagedFile {
  path: string;
  sha1: string;
}

export interface Instance {
  id: string;
  name: string;
  minecraft: string;
  loader: Loader | null;
  source: Source;
  managed: ManagedFile[];
  memory_mb: number | null;
  last_played: LastPlayed | null;
  manual: Manual[];
  skipped: string[];
}

export interface ModFile {
  name: string;
  size: number;
  managed: boolean;
}

export interface Unreadable {
  folder: string;
}

export interface Listing {
  instances: Instance[];
  unreadable: Unreadable[];
}

export interface Progress {
  step: number;
  steps: number;
  stage: string;
  done: number;
  total: number;
}

export interface GameExited {
  instance: string;
  code: number | null;
}

export const instances = () => invoke<Listing>("instances");
export const instanceGet = (id: string) => invoke<Instance>("instance_get", { id });
export const instanceMods = (id: string) => invoke<ModFile[]>("instance_mods", { id });
export const instanceCreate = (name: string, minecraft: string, loader: string | null) =>
  invoke<Instance>("instance_create", { name, minecraft, loader });
export const instanceDelete = (id: string) => invoke<void>("instance_delete", { id });
export const instanceAddMod = (id: string, path: string) => invoke<void>("instance_add_mod", { id, path });
export const instanceRemoveMod = (id: string, name: string) => invoke<void>("instance_remove_mod", { id, name });
export const instanceSetMemory = (id: string, mb: number) => invoke<Instance>("instance_set_memory", { id, mb });
export const instanceCheckManual = (id: string) => invoke<Instance>("instance_check_manual", { id });
export const instanceLaunch = (id: string) => invoke<Instance>("instance_launch", { id });
export const join = (panelId: string, slug: string) => invoke<Instance>("join", { panelId, slug });
export const minecraftVersions = () => invoke<string[]>("minecraft_versions");
export const openModsFolder = (id: string) => invoke<void>("open_mods_folder", { id });
export const openInstanceFolder = (id: string) => invoke<void>("open_instance_folder", { id });

export const onProgress = (cb: (p: Progress) => void): Promise<UnlistenFn> =>
  listen<Progress>("progress", (e) => cb(e.payload));
export const onGameExited = (cb: (e: GameExited) => void): Promise<UnlistenFn> =>
  listen<GameExited>("game-exited", (e) => cb(e.payload));
