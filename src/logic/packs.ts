import type { PackVersion } from "../api";
import { loaderName } from "./servers";

export function packVersionText(v: PackVersion): string {
  const loaders = v.loaders.filter((l) => l !== "quilt").map(loaderName);
  const kind = v.version_type && v.version_type !== "release" ? ` (${v.version_type})` : "";
  return [`${v.name}${kind}`, v.game_versions.length > 0 && `Minecraft ${v.game_versions.join(", ")}`, ...loaders].filter(Boolean).join(" · ");
}

// The newest release; a pack that has only betas gives its newest.
export const defaultPackVersion = (list: PackVersion[]): string => (list.find((v) => v.version_type === "release") ?? list[0])?.id ?? "";
