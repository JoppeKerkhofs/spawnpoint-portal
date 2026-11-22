import { expect, test } from "vitest";
import type { PanelServers, Players, Server } from "../api";
import type { Playing } from "./operation";
import { detailRows, findServer, instanceFor, joinButton, joinNote, mayPoll, playersLine, playersText, runsText, serverSummaryLine, serversState, serversSummary } from "./servers";
import { minecraftText } from "./format";

const srv = (extra: Partial<Server> = {}): Server => ({
  slug: "s", name: "S", hostname: "s.example", state: "running", game_version: "26.3", modpack: null, loader: null, players: null, picture: null, detail: null, ...extra,
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
  expect(p({ online: 3, max: 20, names: [] })).toBe("3 of 20 online"); expect(p({ online: 0, max: 20, names: [] })).toBe("Nobody online"); expect(p(null)).toBe("—");
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

test("which players text applies", () => {
  const on = (online: number, names: string[], state = "running") => srv({ state, players: { online, max: 20, names } });
  expect(playersLine(on(0, [], "offline"))).toBe("The server is not running.");
  expect(playersLine(on(2, ["a", "b"], "installing"))).toBe("The server is not running.");
  expect(playersLine(on(0, []))).toBe("Nobody is online.");
  expect(playersLine(on(2, []))).toBe("This panel does not say who is online.");
  expect(playersLine(srv({ players: null }))).toBe("This panel does not say who is online.");
  expect(playersLine(on(2, ["a", "b"]))).toBeNull();
});

test("finding the open server and its instance", () => {
  const l = [joppe(), lienBack()];
  l[1].servers[0].slug = "x";
  expect(findServer(l, "lien", "x")).toMatchObject({ panel: { id: "lien" }, server: { slug: "x" } });
  expect(findServer(l, "lien", "gone")).toBeNull();
  expect(findServer(null, "lien", "x")).toBeNull();
  const inst = (id: string, panel: string, server: string) => ({ id, source: { kind: "server", panel, server, pack: null } }) as never;
  const own = { id: "o", source: { kind: "own" } } as never;
  expect(instanceFor([own, inst("a", "lien", "y"), inst("b", "lien", "x")], "lien", "x")).toBe("b");
  expect(instanceFor([own], "lien", "x")).toBeNull();
});

test("summary line and details rows", () => {
  const p = { id: "lien", address: "https://panel.lien.example:8443/base", name: "Lien" };
  const s = srv({ hostname: "play.example", game_version: "1.21", loader: { name: "neoforge", version: "21.1" } });
  expect(serverSummaryLine(s, p)).toBe("Minecraft 1.21 · NeoForge 21.1 · panel.lien.example:8443");
  expect(detailRows(s, p)).toEqual([
    ["Address", "play.example"], ["Minecraft version", "1.21"], ["Loader", "NeoForge 21.1"], ["Panel", "panel.lien.example:8443"],
  ]);
  const pack = srv({ modpack: { catalogue: "ftb", project: "x", version: null, version_name: "1.3" } });
  expect(detailRows(pack, p).map((r) => r[0])).toEqual(["Address", "Minecraft version", "Modpack", "Panel"]);
  expect(detailRows(pack, p)[2][1]).toBe("FTB 1.3");
});

test("a refresh nobody asked for waits for a game, a join, a hidden window and the floor", () => {
  expect(mayPoll(0, false, false, false, 0, 0)).toBe(true);
  expect(mayPoll(1, false, false, false, 0, 0)).toBe(false);
  expect(mayPoll(0, true, false, false, 0, 0)).toBe(false);
  expect(mayPoll(0, false, true, false, 0, 0)).toBe(false);
  expect(mayPoll(0, false, false, true, 0, 0)).toBe(false);
  expect(mayPoll(0, false, false, false, 29_000, 30_000)).toBe(false);
  expect(mayPoll(0, false, false, false, 30_000, 30_000)).toBe(true);
});
