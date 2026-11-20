import { ref, type Ref } from "vue";

export type Theme = "default" | "blocky";
export type Appearance = "light" | "dark" | "system";
export interface Look {
  theme: Theme;
  appearance: Appearance;
}

const themes: Theme[] = ["default", "blocky"];
const appearances: Appearance[] = ["light", "dark", "system"];

function pick<T extends string>(read: () => string | null, allowed: T[], fallback: T): T {
  try {
    const v = read();
    return allowed.find((a) => a === v) ?? fallback;
  } catch {
    return fallback;
  }
}

export function readLook(storage: Pick<Storage, "getItem">): Look {
  return {
    theme: pick(() => storage.getItem("theme"), themes, "default"),
    appearance: pick(() => storage.getItem("appearance"), appearances, "system"),
  };
}

export function saveLook(storage: Pick<Storage, "setItem">, l: Look): void {
  try {
    storage.setItem("theme", l.theme);
    storage.setItem("appearance", l.appearance);
  } catch {
    // storage may be blocked; the look then lasts until the window closes
  }
}

export function applyLook(
  l: Look,
  root: { dataset: DOMStringMap; classList: Pick<DOMTokenList, "toggle"> },
  prefersDark: boolean,
): void {
  root.dataset.theme = l.theme;
  const dark = l.theme === "default" && (l.appearance === "dark" || (l.appearance === "system" && prefersDark));
  root.classList.toggle("dark", dark);
}

const query = () => matchMedia("(prefers-color-scheme: dark)");

export const look: Ref<Look> = ref({ theme: "default", appearance: "system" });

export function setLook(l: Look): void {
  look.value = l;
  saveLook(localStorage, l);
  applyLook(l, document.documentElement, query().matches);
}

// Called once from main.ts: load the stored look, apply it and follow Windows while appearance is "system".
export function initLook(): void {
  look.value = readLook(localStorage);
  const q = query();
  const apply = () => applyLook(look.value, document.documentElement, q.matches);
  apply();
  q.addEventListener("change", () => look.value.appearance === "system" && apply());
}
