import { describe, expect, test } from "vitest";
import initScript from "../../public/look-init.js?raw";
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

  test("the pre-paint script gives what applyLook gives", () => {
    const themes: (string | null)[] = ["default", "blocky", "pink", null];
    const appearances: (string | null)[] = ["light", "dark", "system", "dim", null];
    for (const theme of themes) {
      for (const appearance of appearances) {
        for (const osDark of [true, false]) {
          const stored: Record<string, string> = {};
          if (theme) stored.theme = theme;
          if (appearance) stored.appearance = appearance;
          const classes = new Set<string>();
          const root = { dataset: {} as DOMStringMap, classList: { add: (c: string) => classes.add(c) } };
          new Function("document", "localStorage", "matchMedia", initScript)({ documentElement: root }, store(stored), () => ({ matches: osDark }));

          const want = fakeRoot();
          applyLook(readLook(store(stored)), want, osDark);
          expect([root.dataset.theme, classes.has("dark")], `${theme} ${appearance} ${osDark}`).toEqual([want.dataset.theme, want.dark()]);
        }
      }
    }
  });
});
