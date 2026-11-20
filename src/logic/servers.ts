import type { PanelServers, Players, Server } from "../api";
import { upper } from "./format";
import type { Op, Playing } from "./operation";

export type ServersState = "loading" | "none-linked" | "none-answered" | "filled";

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
const loaders: Record<string, string> = { fabric: "Fabric", neoforge: "NeoForge", forge: "Forge" };

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
