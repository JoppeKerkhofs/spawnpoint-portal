import type { Linked, PanelServers } from "../api";
import { andList, errorText } from "./format";

export const looksLikeAddress = (s: string): boolean => /^https?:\/\/[^\s/?#.:]+(\.[^\s/?#.:]+)+(:\d+)?([/?#]\S*)?$/i.test(s.trim());

export const needsConsent = (s: string): boolean => /^http:\/\//i.test(s.trim());

export const canGetCode = (address: string, consent: boolean): boolean =>
  looksLikeAddress(address) && (!needsConsent(address) || consent);

export type PollVerdict = "continue" | "unreachable" | "linked" | "expired" | "failed";

// An unreachable panel is not an expiry: polling goes on and the view says so.
export function pollVerdict(
  now: number,
  expiresAt: number,
  result: { linked?: Linked | null; error?: unknown } | null,
): PollVerdict {
  if (result?.linked) return "linked";
  if (now >= expiresAt) return "expired";
  if (result?.error) return (result.error as { kind?: string }).kind === "unreachable" ? "unreachable" : "failed";
  return "continue";
}

export function linkedStatus(entry: PanelServers | undefined): { ok: boolean; text: string } | null {
  if (!entry) return null;
  if (entry.error) return { ok: false, text: errorText(entry.error) };
  const n = entry.servers.length;
  return { ok: true, text: `Connected · ${n} ${n === 1 ? "server" : "servers"}` };
}

export const missingText = (missing: string[]): string =>
  `The key lacks these permissions: ${andList(missing)}. Grant them on the panel’s token page.`;
