import { expect, test } from "vitest";
import type { PanelServers, Players, Server } from "../api";
import type { Playing } from "./operation";
import { joinButton, joinNote, playersText, runsText, serversState, serversSummary } from "./servers";
import { minecraftText } from "./format";

const srv = (extra: Partial<Server> = {}): Server => ({
  slug: "s", name: "S", hostname: "s.example", state: "running", game_version: "26.3", modpack: null, loader: null, players: null, ...extra,
});
const panel = (id: string) => ({ id, address: `https://${id}.example`, name: id });
const joppe = (): PanelServers => ({
  panel: panel("joppe"), error: null,
  servers: [srv({ state: "running" }), srv({ state: "running" }), srv({ state: "offline" }), srv({ state: "installing" }), srv({ state: "error" })],
});
const lienBack = (): PanelServers => ({ panel: panel("lien"), error: null, servers: [srv({ state: "running" })] });
const failed = (id: string): PanelServers => ({ panel: panel(id), servers: [], error: { kind: "unreachable", message: "x" } });
const playingOn = (serverKey: string): Playing => ({ instance: "i", name: "N", since: new Date(), host: null, memoryMb: 4096, serverKey });

test("state and summary", () => {
  expect(serversState(null)).toBe("loading"); expect(serversSummary(null)).toBe("Asking your panels what they run…");
  expect(serversState([])).toBe("none-linked"); expect(serversSummary([])).toBe("Nothing linked yet");
  expect(serversState([failed("a"), failed("b")])).toBe("none-answered"); expect(serversSummary([failed("a"), failed("b")])).toBe("0 of 2 panels answered");
  expect(serversState([joppe(), failed("lien")])).toBe("filled");
  expect(serversSummary([joppe(), failed("lien")])).toBe("5 servers from 1 of 2 panels · 2 running");
  expect(serversSummary([joppe(), lienBack()])).toBe("6 servers from 2 panels · 3 running");
  expect(serversSummary([lienBack()])).toBe("1 server from 1 panel · 1 running");
});

test("a failed first ask leaves the summary empty", () => {
  expect(serversSummary(null, true)).toBe("");
  expect(serversSummary([lienBack()], true)).toBe("1 server from 1 panel · 1 running");
});

test("an unknown game version is a dash", () => {
  expect(minecraftText("26.3")).toBe("Minecraft 26.3");
  expect(minecraftText(null)).toBe("—");
  expect(minecraftText("")).toBe("—");
});

test("what a server runs", () => {
  expect(runsText(srv({ modpack: { catalogue: "modrinth", project: "x", version: "1", version_name: "1.3.7" } }))).toBe("Modrinth pack 1.3.7");
  expect(runsText(srv({ modpack: { catalogue: "ftb", project: "x", version: null, version_name: null } }))).toBe("FTB pack");
  expect(runsText(srv({ loader: { name: "fabric", version: "0.20.1" } }))).toBe("Fabric 0.20.1");
  expect(runsText(srv({ loader: { name: "forge", version: null } }))).toBe("Forge (newest)");
  expect(runsText(srv({}))).toBe("No mod loader");
  const p = (x: Players | null) => playersText(x);
  expect(p({ online: 3, max: 20 })).toBe("3 of 20 online"); expect(p({ online: 0, max: 20 })).toBe("Nobody online"); expect(p(null)).toBe("—");
});

test("join button", () => {
  const idle = { phase: "idle" } as const;
  expect(joinButton(srv({ name: "Survival" }), "p/survival", idle, null, [])).toEqual({ label: "Join", aria: "Join Survival", disabled: false, hint: "" });
  expect(joinButton(srv({ state: "installing" }), "p/atm10", idle, null, [])).toMatchObject({ disabled: true, hint: "Still installing its pack." });
  expect(joinButton(srv({ state: "offline" }), "p/home", idle, null, [])).toMatchObject({ disabled: false, hint: "Server is off. You can still sync." });
  const prep = { phase: "preparing", name: "Survival", step: 1, steps: 6, stage: "check", done: 0, total: 0 } as const;
  expect(joinButton(srv({ name: "Survival" }), "p/survival", prep, "p/survival", [])).toEqual({ label: "Preparing…", aria: "Preparing… Survival", disabled: true, hint: "Already starting" });
  expect(joinButton(srv({ name: "Main" }), "p/main", prep, "p/survival", [])).toMatchObject({ label: "Join", disabled: true });
  const starting = { phase: "starting", name: "Survival" } as const;
  expect(joinButton(srv({ name: "Survival" }), "p/survival", starting, "p/survival", [])).toMatchObject({ label: "Starting…", disabled: true, hint: "Already starting" });
  expect(joinButton(srv({ name: "Main" }), "p/main", idle, null, [playingOn("p/main")])).toMatchObject({ label: "Playing", disabled: true });
  expect(joinButton(srv({ name: "Survival" }), "p/survival", idle, null, [playingOn("p/main")])).toMatchObject({ label: "Join", disabled: false });
});

test("a game that was not given the server says so after the join", () => {
  expect(joinNote(true)).toBeNull();
  expect(joinNote(false)).toBe("This Minecraft version can't join a server by itself. Add the server in the game's multiplayer list.");
});
