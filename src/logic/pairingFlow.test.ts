import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { Linked, PairingView } from "../api";
import { pairingFlow } from "./pairingFlow";

const T0 = Date.parse("2026-11-20T12:00:00Z");
const view = (pairing: number, ttl = 60): PairingView => ({
  pairing, code: "ABCD", address: "https://p.example", approve_at: "https://p.example/approve", expires_at: new Date(T0 + ttl * 1000).toISOString(), interval: 2,
});
const linked: Linked = { panel: { id: "p", address: "https://p.example", name: "p" }, missing: [] };

function setup(answers: Array<() => Promise<Linked | null>> = []) {
  const log: string[] = [];
  const polls: number[] = [];
  const calls = {
    start: vi.fn(async () => view(7)),
    poll: vi.fn(async (n: number) => {
      polls.push(n);
      return (answers.shift() ?? (async () => null))();
    }),
    cancel: vi.fn(async () => {}),
  };
  const events = {
    code: vi.fn((p: PairingView) => log.push(`code ${p.pairing}`)),
    tick: vi.fn(),
    unreachable: vi.fn((v: boolean) => log.push(`unreachable ${v}`)),
    linked: vi.fn(() => log.push("linked")),
    expired: vi.fn((e: unknown) => log.push(`expired ${e ? "error" : "plain"}`)),
  };
  return { flow: pairingFlow(calls, events), calls, events, log, polls };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(T0);
});
afterEach(() => vi.useRealTimers());

test("a started pairing polls every interval until the panel says linked", async () => {
  const s = setup([async () => null, async () => linked]);
  await s.flow.start("https://p.example", false);
  expect(s.calls.start).toHaveBeenCalledWith("https://p.example", false);
  expect(s.log).toEqual(["code 7"]);
  await vi.advanceTimersByTimeAsync(1999);
  expect(s.polls).toEqual([]);
  await vi.advanceTimersByTimeAsync(1);
  expect(s.polls).toEqual([7]);
  await vi.advanceTimersByTimeAsync(2000);
  expect(s.log).toEqual(["code 7", "unreachable false", "unreachable false", "linked"]);
  const seen = s.events.tick.mock.calls.length;
  await vi.advanceTimersByTimeAsync(10000);
  expect(s.polls).toEqual([7, 7]);
  expect(s.events.tick.mock.calls.length).toBe(seen);
  s.flow.leave();
  expect(s.calls.cancel).not.toHaveBeenCalled();
});

test("the code expires: the panel is told, polling stops", async () => {
  const s = setup();
  s.calls.start.mockResolvedValue(view(7, 5));
  await s.flow.start("https://p.example", false);
  await vi.advanceTimersByTimeAsync(5000);
  expect(s.log[s.log.length - 1]).toBe("expired plain");
  expect(s.calls.cancel).toHaveBeenCalledWith(7);
  const polled = s.polls.length;
  await vi.advanceTimersByTimeAsync(20000);
  expect(s.polls.length).toBe(polled);
  expect(s.calls.cancel).toHaveBeenCalledTimes(1);
});

test("an unreachable panel keeps being polled, any other error ends it", async () => {
  const s = setup([
    () => Promise.reject({ kind: "unreachable", message: "x" }),
    async () => null,
    () => Promise.reject({ kind: "other", message: "used" }),
  ]);
  await s.flow.start("https://p.example", false);
  await vi.advanceTimersByTimeAsync(6000);
  expect(s.log).toEqual(["code 7", "unreachable true", "unreachable false", "unreachable false", "expired error"]);
  expect(s.events.expired).toHaveBeenCalledWith({ kind: "other", message: "used" });
  expect(s.calls.cancel).toHaveBeenCalledWith(7);
});

test("cancelling stops the polls and the timer and tells the panel", async () => {
  const s = setup();
  await s.flow.start("https://p.example", false);
  s.flow.cancel();
  expect(s.calls.cancel).toHaveBeenCalledWith(7);
  await vi.advanceTimersByTimeAsync(30000);
  expect(s.polls).toEqual([]);
  expect(s.events.tick).not.toHaveBeenCalled();
});

test("leaving the view before the start answers cancels that pairing and shows nothing", async () => {
  const s = setup();
  let answer!: (p: PairingView) => void;
  s.calls.start.mockReturnValue(new Promise((r) => (answer = r)));
  const started = s.flow.start("https://p.example", false);
  s.flow.leave();
  answer(view(9));
  await started;
  expect(s.calls.cancel).toHaveBeenCalledWith(9);
  expect(s.events.code).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(30000);
  expect(s.polls).toEqual([]);
});

test("leaving the view while a code shows cancels it", async () => {
  const s = setup();
  await s.flow.start("https://p.example", false);
  s.flow.leave();
  expect(s.calls.cancel).toHaveBeenCalledWith(7);
  await vi.advanceTimersByTimeAsync(30000);
  expect(s.polls).toEqual([]);
});

test("a late answer from an abandoned pairing is ignored", async () => {
  let late!: (l: Linked | null) => void;
  const s = setup([() => new Promise((r) => (late = r))]);
  await s.flow.start("https://p.example", false);
  await vi.advanceTimersByTimeAsync(2000);
  expect(s.polls).toEqual([7]);
  s.flow.cancel();
  s.log.length = 0;
  late(linked);
  await vi.advanceTimersByTimeAsync(10000);
  expect(s.log).toEqual([]);
  expect(s.polls).toEqual([7]);
});

test("a new pairing replaces the old one's polling", async () => {
  let late!: (l: Linked | null) => void;
  const s = setup([() => new Promise((r) => (late = r))]);
  await s.flow.start("https://p.example", false);
  await vi.advanceTimersByTimeAsync(2000);
  s.calls.start.mockResolvedValue(view(8));
  await s.flow.start("https://p.example", false);
  late(linked);
  await vi.advanceTimersByTimeAsync(2000);
  expect(s.log).toEqual(["code 7", "code 8", "unreachable false"]);
  expect(s.polls).toEqual([7, 8]);
});
