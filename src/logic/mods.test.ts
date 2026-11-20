import { expect, test } from "vitest";
import type { Instance, ModFile, Source } from "../api";
import { deleteBody, filterMods, handHeading, instanceSummary, isHttps, memoryLabel, memoryOptions, modCounts, modsText, noModsText, orderMods, skippedName } from "./mods";

const m = (name: string, managed: boolean): ModFile => ({ name, size: 1, managed });
const inst = (extra: Partial<Instance> = {}): Instance => ({
  id: 'i', name: 'N', minecraft: '1.20.1', loader: null, source: { kind: 'manual' } as Source, managed: [], memory_mb: null,
  last_played: null, manual: [], skipped: [], ...extra,
});
const panels = [{ id: 'panel-joppek-be', address: 'https://panel.joppek.be', name: 'Joppe’s Spawnpoint' }]
const homestead = () => inst({
  name: 'Homestead', loader: { kind: 'fabric', version: '0.18.4' },
  source: { kind: 'server', panel: 'panel-joppek-be', server: 'homestead', pack: { catalogue: 'modrinth', project: 'x', version: 'y', version_name: '1.3.7' } },
})

test('order, filter and counts', () => {
  const mods = [m('appleskin.jar', true), m('iris-mc1.20.1-1.7.2.jar', false), m('Sodium.jar', true), m('journeymap.jar', false)]
  expect(orderMods(mods).map((x) => x.name)).toEqual(['iris-mc1.20.1-1.7.2.jar', 'journeymap.jar', 'appleskin.jar', 'Sodium.jar'])
  expect(filterMods(mods, 'all', 'SOD').map((x) => x.name)).toEqual(['Sodium.jar'])
  expect(filterMods(mods, 'own', '')).toHaveLength(2)
  expect(filterMods(mods, 'pack', 'iris')).toHaveLength(0)
  expect(modCounts(mods)).toEqual({ all: 4, pack: 2, own: 2 })
  expect(modsText(1)).toBe('1 mod'); expect(modsText(216)).toBe('216 mods')
  expect(noModsText('xyz')).toBe('No mod matches “xyz”.'); expect(noModsText('')).toBe('No mods here.')
})
test('texts', () => {
  expect(instanceSummary(homestead(), panels)).toBe('Minecraft 1.20.1 · Fabric 0.18.4 · Modrinth pack 1.3.7 · from Homestead on Joppe’s Spawnpoint')
  expect(instanceSummary(inst(), panels)).toBe('Minecraft 1.20.1')
  expect(handHeading(1)).toBe('1 file to download by hand'); expect(handHeading(3)).toBe('3 files to download by hand')
  expect(skippedName('mods/iris-mc1.20.1-1.7.2.jar')).toBe('iris-mc1.20.1-1.7.2.jar'); expect(skippedName('config/x.toml')).toBe('config/x.toml')
  expect(deleteBody(homestead(), [m('a.jar', true), m('b.jar', false)])).toBe('This removes the whole instance folder: 2 mods (including your 1 own), its configs, and its saved worlds. It can’t be undone. You can always join the Homestead server again to get a fresh copy.')
  expect(deleteBody(inst({ name: 'Old times', source: { kind: 'manual' } }), [m('a.jar', true)])).toBe('This removes the whole instance folder: 1 mod, its configs, and its saved worlds. It can’t be undone.')
})
test('links and memory', () => {
  expect(isHttps('https://www.curseforge.com/x')).toBe(true); expect(isHttps('http://x')).toBe(false); expect(isHttps('javascript:alert(1)')).toBe(false); expect(isHttps('')).toBe(false)
  expect(memoryLabel(4096)).toBe('4 GB'); expect(memoryLabel(10240)).toBe('10 GB'); expect(memoryLabel(1536)).toBe('1.5 GB')
  expect(memoryOptions([4096, 6144, 8192, 10240], 6144)).toEqual([4096, 6144, 8192, 10240])
  expect(memoryOptions([4096, 6144], 5120)).toEqual([4096, 5120, 6144])
})
