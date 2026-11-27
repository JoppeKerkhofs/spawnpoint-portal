import { expect, test } from "vitest";
import type { PackVersion } from "../api";
import { defaultPackVersion, packVersionText } from "./packs";

const v = (id: string, version_type: string, loaders = ["fabric"]): PackVersion => ({ id, name: `${id}.0`, game_versions: ["1.21.1"], loaders, version_type });

test("a pack version reads as its number, game version and loader", () => {
  expect(packVersionText(v("2", "release"))).toBe("2.0 · Minecraft 1.21.1 · Fabric");
  expect(packVersionText(v("3", "beta", ["quilt", "neoforge"]))).toBe("3.0 (beta) · Minecraft 1.21.1 · NeoForge");
  expect(packVersionText({ ...v("1", ""), game_versions: [], loaders: [] })).toBe("1.0");
});

test("the newest release is chosen first, a beta only when there is nothing else", () => {
  expect(defaultPackVersion([v("3", "beta"), v("2", "release"), v("1", "release")])).toBe("2");
  expect(defaultPackVersion([v("3", "beta")])).toBe("3");
  expect(defaultPackVersion([])).toBe("");
});
