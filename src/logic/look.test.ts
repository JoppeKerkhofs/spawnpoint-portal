import { describe, expect, test } from "vitest";
import { applyLook, readLook } from "./look";

const store = (m: Record<string, string>) => ({ getItem: (k: string) => m[k] ?? null });
const fakeRoot = () => {
  let dark = false;
  return {
    dataset: {} as DOMStringMap,
    classList: { toggle: (_: string, on?: boolean) => ((dark = !!on), !!on) },
    dark: () => dark,
  };
};

describe("look", () => {
  test("defaults and junk fall back", () => {
    expect(readLook(store({}))).toEqual({ theme: "default", appearance: "system" });
    expect(readLook(store({ theme: "pink", appearance: "dim" }))).toEqual({ theme: "default", appearance: "system" });
    expect(readLook(store({ theme: "blocky", appearance: "dark" }))).toEqual({ theme: "blocky", appearance: "dark" });
    expect(
      readLook({
        getItem: () => {
          throw new Error("denied");
        },
      }),
    ).toEqual({ theme: "default", appearance: "system" });
  });
  test("system follows Windows, blocky is never dark", () => {
    const r = fakeRoot();
    applyLook({ theme: "default", appearance: "system" }, r, true);
    expect([r.dataset.theme, r.dark()]).toEqual(["default", true]);
    applyLook({ theme: "default", appearance: "light" }, r, true);
    expect(r.dark()).toBe(false);
    applyLook({ theme: "blocky", appearance: "dark" }, r, true);
    expect([r.dataset.theme, r.dark()]).toEqual(["blocky", false]);
  });
});
