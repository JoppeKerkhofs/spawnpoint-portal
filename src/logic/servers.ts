import { ref } from "vue";
import { join, servers, type CommandError, type Instance, type Panel, type PanelServers, type Players, type Server } from "../api";
import { minecraftText, upper } from "./format";
import { afterJoin, markPlaying, op, openInstance, playing, withProgress, type Op, type Playing } from "./operation";

// Shared, so leaving and re-entering a view shows the earlier answer at once and a refresh updates every view.
export const list = ref<PanelServers[] | null>(null);
export const loading = ref(false);
export const loadError = ref<CommandError | null>(null);

export async function loadServers(): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  try {
    list.value = await servers();
    loadError.value = null;
  } catch (e) {
    loadError.value = e as CommandError;
  } finally {
    loading.value = false;
  }
}

export const POLL_MS = 120_000;
const FOCUS_FLOOR_MS = 30_000;

// A refresh nobody asked for waits while a game is open, something is being prepared, or the window cannot be seen.
export const mayPoll = (games: number, working: boolean, hidden: boolean, minimized: boolean, sinceLastMs: number, floorMs: number) =>
  games === 0 && !working && !hidden && !minimized && sinceLastMs >= floorMs;

// Refreshes the shared list every two minutes and when the window gets the focus back; returns how to stop.
export function pollServers(minimized: () => Promise<boolean>): () => void {
  let last = Date.now();
  const tick = async (floorMs: number) => {
    const small = await minimized().catch(() => false);
    if (!mayPoll(playing.value.length, op.value.phase !== "idle", document.hidden, small, Date.now() - last, floorMs)) return;
    last = Date.now();
    await loadServers();
  };
  const onFocus = () => void tick(FOCUS_FLOOR_MS);
  const timer = setInterval(() => void tick(0), POLL_MS);
  window.addEventListener("focus", onFocus);
  return () => {
    clearInterval(timer);
    window.removeEventListener("focus", onFocus);
  };
}

export async function joinServer(panel: Panel, s: Server): Promise<boolean> {
  const done = await withProgress(s.name, () => join(panel.id, s.slug), serverKey(panel.id, s.slug));
  if (!done) return false;
  const { instance, joined } = done;
  markPlaying(instance, s.hostname);
  const text = joinNote(joined);
  afterJoin.value = text ? { instance: instance.id, text } : null;
  if (instance.manual.length > 0 || text) openInstance(instance.id);
  return true;
}

export const findServer = (l: PanelServers[] | null, panelId: string, slug: string): { panel: Panel; server: Server } | null => {
  const entry = l?.find((p) => p.panel.id === panelId);
  const server = entry?.servers.find((s) => s.slug === slug);
  return entry && server ? { panel: entry.panel, server } : null;
};

export const instanceFor = (l: Instance[], panelId: string, slug: string): string | null =>
  l.find((i) => i.source.kind === "server" && i.source.panel === panelId && i.source.server === slug)?.id ?? null;

export function playersLine(s: Server): string | null {
  if (s.state !== "running") return "The server is not running.";
  if (s.players?.names.length) return null;
  return s.players?.online === 0 ? "Nobody is online." : "This panel does not say who is online.";
}

const panelHost = (p: Panel) => p.address.split("://").pop()!.split("/")[0];

export const serverSummaryLine = (s: Server, p: Panel) => `${minecraftText(s.game_version)} · ${runsText(s)} · ${panelHost(p)}`;

export function detailRows(s: Server, p: Panel): Array<[string, string]> {
  const rows: Array<[string, string | null]> = [
    ["Address", s.hostname],
    ["Minecraft version", s.game_version ?? "—"],
    ["Loader", s.loader && `${loaderName(s.loader.name)} ${s.loader.version ?? "(newest)"}`],
    ["Modpack", s.modpack && `${catalogueName(s.modpack.catalogue)}${s.modpack.version_name ? ` ${s.modpack.version_name}` : ""}`],
    ["Panel", panelHost(p)],
  ];
  return rows.filter((r): r is [string, string] => r[1] !== null);
}

export type ServersState = "loading" | "none-linked" | "none-answered" | "filled";

export const joinNote = (joined: boolean): string | null =>
  joined ? null : "This Minecraft version can't join a server by itself. Add the server in the game's multiplayer list.";

export const serverKey = (panelId: string, slug: string) => `${panelId}/${slug}`;

export function serversState(list: PanelServers[] | null): ServersState {
  if (!list) return "loading";
  if (list.length === 0) return "none-linked";
  return list.every((p) => p.error) ? "none-answered" : "filled";
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

export function serversSummary(list: PanelServers[] | null, failed = false): string {
  switch (serversState(list)) {
    case "loading":
      return failed ? "" : "Asking your panels what they run…";
    case "none-linked":
      return "Nothing linked yet";
    case "none-answered":
      return `0 of ${list!.length} panels answered`;
  }
  const answered = list!.filter((p) => !p.error);
  const all = answered.length === list!.length;
  const servers = answered.flatMap((p) => p.servers);
  const running = servers.filter((s) => s.state === "running").length;
  const panels = all ? plural(list!.length, "panel") : `${answered.length} of ${list!.length} panels`;
  return `${plural(servers.length, "server")} from ${panels} · ${running} running`;
}

export function playersText(p: Players | null): string {
  if (!p) return "—";
  return p.online === 0 ? "Nobody online" : `${p.online} of ${p.max} online`;
}

const catalogues: Record<string, string> = { ftb: "FTB", modrinth: "Modrinth", curseforge: "CurseForge" };
const loaders: Record<string, string> = { fabric: "Fabric", neoforge: "NeoForge", forge: "Forge", horizon: "Horizon" };

export const catalogueName = (c: string) => catalogues[c] ?? upper(c);
export const loaderName = (l: string) => loaders[l] ?? upper(l);

export function runsText(s: Server): string {
  if (s.modpack) return `${catalogueName(s.modpack.catalogue)} pack${s.modpack.version_name ? ` ${s.modpack.version_name}` : ""}`;
  if (s.loader) return `${loaderName(s.loader.name)} ${s.loader.version ?? "(newest)"}`;
  return "No mod loader";
}

export function joinButton(
  s: Server,
  key: string,
  op: Op,
  opKey: string | null,
  playing: Playing[],
): { label: string; aria: string; disabled: boolean; hint: string } {
  const working = op.phase === "preparing" || op.phase === "starting";
  const mine = working && opKey === key;
  const stateHint = s.state === "installing" ? "Still installing its pack." : s.state === "offline" ? "Server is off. You can still sync." : "";
  let label = "Join";
  let hint = stateHint;
  if (mine) {
    label = op.phase === "preparing" ? "Preparing…" : "Starting…";
    hint = "Already starting";
  } else if (playing.some((p) => p.serverKey === key)) {
    label = "Playing";
    hint = "";
  }
  return {
    label,
    aria: `${label} ${s.name}`,
    disabled: working || label === "Playing" || s.state === "installing",
    hint,
  };
}
