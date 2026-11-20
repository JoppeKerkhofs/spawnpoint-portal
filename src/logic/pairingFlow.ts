import type { CommandError, Linked, PairingView } from "../api";
import { pollVerdict } from "./pairing";

export interface PairingCalls {
  start(address: string, allowHttp: boolean): Promise<PairingView>;
  poll(pairing: number): Promise<Linked | null>;
  cancel(pairing: number): Promise<unknown>;
}

export interface PairingEvents {
  code(p: PairingView): void;
  tick(now: number): void;
  unreachable(v: boolean): void;
  linked(l: Linked): void;
  expired(error: CommandError | null): void;
}

export function pairingFlow(calls: PairingCalls, events: PairingEvents, now: () => number = Date.now) {
  // Each attempt owns a number; a late poll answer from an older one is ignored.
  let session = 0;
  let gone = false;
  let current: number | null = null;
  let poller: ReturnType<typeof setTimeout> | undefined;
  let ticker: ReturnType<typeof setInterval> | undefined;

  function halt() {
    session++;
    clearTimeout(poller);
    clearInterval(ticker);
  }

  function cancel() {
    halt();
    if (current !== null) calls.cancel(current).catch(() => {});
    current = null;
  }

  function schedule(mine: number, pairing: number, expiresAt: number, interval: number) {
    poller = setTimeout(async () => {
      let result: { linked?: Linked | null; error?: unknown };
      try {
        result = { linked: await calls.poll(pairing) };
      } catch (e) {
        result = { error: e };
      }
      if (mine !== session) return;
      const verdict = pollVerdict(now(), expiresAt, result);
      events.unreachable(verdict === "unreachable");
      if (verdict === "continue" || verdict === "unreachable") schedule(mine, pairing, expiresAt, interval);
      else if (verdict === "linked") {
        halt();
        current = null;
        events.linked(result.linked!);
      } else {
        cancel();
        events.expired(verdict === "failed" ? (result.error as CommandError) : null);
      }
    }, interval * 1000);
  }

  async function start(address: string, allowHttp: boolean): Promise<void> {
    const p = await calls.start(address, allowHttp);
    if (gone) return void calls.cancel(p.pairing).catch(() => {});
    halt();
    const mine = session;
    current = p.pairing;
    const expiresAt = Date.parse(p.expires_at);
    events.code(p);
    ticker = setInterval(() => {
      events.tick(now());
      if (pollVerdict(now(), expiresAt, null) === "expired") {
        cancel();
        events.expired(null);
      }
    }, 1000);
    schedule(mine, p.pairing, expiresAt, p.interval);
  }

  return {
    start,
    cancel,
    leave() {
      gone = true;
      cancel();
    },
  };
}
