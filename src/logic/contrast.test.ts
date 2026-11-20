import { expect, test } from "vitest";

// ?raw of a .css file comes back empty under vitest (the css plugins), so read it from disk.
const fs = (globalThis as any).process.getBuiltinModule("node:fs");
const css: string = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8");

function tokens(header: string): Record<string, string> {
  const start = css.indexOf(header);
  expect(start, header).toBeGreaterThan(-1);
  const body = css.slice(start, css.indexOf("\n}", start));
  return Object.fromEntries([...body.matchAll(/(--[\w-]+):\s*(#[0-9a-f]{6})\s*;/gi)].map((m) => [m[1], m[2]]));
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const light = tokens('html[data-theme="default"] {');
const dark = { ...light, ...tokens('html[data-theme="default"].dark {') };

test.each([
  ["light focus ring", light, "--focus", ["--bg", "--surface"]],
  ["light input and segmented border", light, "--input-border", ["--bg", "--surface"]],
  ["dark secondary button border", dark, "--secondary-border", ["--bg", "--surface"]],
  ["dark input and segmented border", dark, "--input-border", ["--bg", "--surface"]],
  ["dark focus ring", dark, "--focus", ["--bg", "--surface"]],
] as const)("%s has 3:1 against the page and the cards", (_, set, token, grounds) => {
  for (const ground of grounds) expect(ratio(set[token], set[ground]), `${token} on ${ground}`).toBeGreaterThanOrEqual(3);
});
