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

export interface Panel {
  id: string;
  address: string;
  name: string;
}

export interface Modpack {
  catalogue: string;
  project: string;
  version: string | null;
  version_name: string | null;
}

export interface ServerLoader {
  name: string;
  version: string | null;
}

export interface Players {
  online: number;
  max: number;
}

export interface Server {
  slug: string;
  name: string;
  hostname: string;
  state: string;
  game_version: string | null;
  modpack: Modpack | null;
  loader: ServerLoader | null;
  players: Players | null;
}

export interface PanelServers {
  panel: Panel;
  servers: Server[];
  error: CommandError | null;
}

export interface PairingView {
  code: string;
  address: string;
  approve_at: string;
  expires_at: string;
  interval: number;
}

export interface Settings {
  player_name: string;
}

export const servers = () => invoke<PanelServers[]>("servers");
export const panels = () => invoke<Panel[]>("panels");
export const panelPairStart = (address: string, allowHttp: boolean) =>
  invoke<PairingView>("panel_pair_start", { address, allowHttp });
export const openPairingPage = () => invoke<void>("open_pairing_page");
export const panelPairPoll = () => invoke<Panel | null>("panel_pair_poll");
export const panelPairCancel = () => invoke<void>("panel_pair_cancel");
export const panelRemove = (id: string) => invoke<void>("panel_remove", { id });
export const settingsGet = () => invoke<Settings>("settings_get");
export const settingsSet = (playerName: string) => invoke<Settings>("settings_set", { playerName });
export const openPage = (url: string) => invoke<void>("open_page", { url });
