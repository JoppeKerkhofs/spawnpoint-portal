import { expect, test } from "vitest";
import type { Instance, InstalledMod, ModFile, Source } from "../api";
import { memoryLabel } from "./format";
import { browseNote, deleteBody, downloadsText, filterMods, handHeading, instanceSummary, isHttps, installedSentence, memoryOptions, modCounts, modsText, noModsText, noResultsText, orderMods, resultControl, skippedName, updatableMod, modIcon } from "./mods";

const m = (name: string, managed: boolean): ModFile => ({ name, size: 1, managed });
const inst = (extra: Partial<Instance> = {}): Instance => ({
  id: 'i', name: 'N', minecraft: '1.20.1', loader: null, source: { kind: 'manual' } as Source, managed: [], memory_mb: null,
  last_played: null, manual: [], skipped: [], picture: null, own_picture: false, ...extra,
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
test('delete body without a loaded list', () => {
  expect(deleteBody(inst({ name: 'Old times' }), null)).toBe('This removes the whole instance folder: its mods, its configs, and its saved worlds. It can’t be undone.')
  expect(instanceSummary(inst({ minecraft: '' }), panels)).toBe('—')
})
test('links and memory', () => {
  expect(isHttps('https://www.curseforge.com/x')).toBe(true); expect(isHttps('http://x')).toBe(false); expect(isHttps('javascript:alert(1)')).toBe(false); expect(isHttps('')).toBe(false)
  expect(memoryLabel(4096)).toBe('4 GB'); expect(memoryLabel(10240)).toBe('10 GB'); expect(memoryLabel(1536)).toBe('1.5 GB')
  expect(memoryOptions([4096, 6144, 8192, 10240], 6144)).toEqual([4096, 6144, 8192, 10240])
  expect(memoryOptions([4096, 6144], 5120)).toEqual([4096, 5120, 6144])
})

const im = (extra: Partial<InstalledMod>): InstalledMod => ({ file: 'a.jar', managed: false, project: 'P1', title: 'Sodium', icon: null, update: false, disabled: false, ...extra })
test('which control a result gets', () => {
  expect(resultControl('P1', null)).toBe('install')
  expect(resultControl('P1', [])).toBe('install')
  expect(resultControl('P1', [im({ project: 'P2' })])).toBe('install')
  expect(resultControl('P1', [im({})])).toBe('installed')
  expect(resultControl('P1', [im({ update: true })])).toBe('update')
  expect(resultControl('P1', [im({ managed: true })])).toBe('pack')
  expect(resultControl('P1', [im({ update: true }), im({ file: 'b.jar', managed: true })])).toBe('pack')
  expect(resultControl('P1', [im({ file: 'a.jar.disabled', disabled: true })])).toBe('disabled')
  expect(resultControl('P1', [im({ file: 'a.jar.disabled', disabled: true }), im({ file: 'b.jar' })])).toBe('installed')
  expect(resultControl('P1', [im({ file: 'a.jar.disabled', disabled: true }), im({ file: 'b.jar', update: true })])).toBe('update')
  expect(resultControl('P1', [im({ file: 'a.jar.disabled', disabled: true, managed: true })])).toBe('pack')
})
test('which mod gets an Update button', () => {
  const known = [im({ file: 'a.jar', update: true }), im({ file: 'b.jar', update: false }), im({ file: 'c.jar', update: true, project: null }), im({ file: 'p.jar', update: true, managed: true })]
  expect(updatableMod(m('a.jar', false), known)?.project).toBe('P1')
  expect(updatableMod(m('b.jar', false), known)).toBeUndefined()
  expect(updatableMod(m('c.jar', false), known)).toBeUndefined()
  expect(updatableMod(m('p.jar', true), known)).toBeUndefined()
  expect(updatableMod(m('a.jar', false), null)).toBeUndefined()
})
test('which picture a row gets', () => {
  const known = [im({ file: 'a.jar', icon: 'https://cdn.modrinth.com/a.webp' }), im({ file: 'b.jar' }), im({ file: 'p.jar', managed: true, icon: 'https://cdn.modrinth.com/p.webp' })]
  expect(modIcon(m('a.jar', false), known)).toBe('https://cdn.modrinth.com/a.webp')
  expect(modIcon(m('p.jar', true), known)).toBe('https://cdn.modrinth.com/p.webp')
  expect(modIcon(m('b.jar', false), known)).toBeNull()
  expect(modIcon(m('c.jar', false), known)).toBeNull()
  expect(modIcon(m('a.jar', false), null)).toBeNull()
})
test('what installing says', () => {
  expect(installedSentence({ installed: ['Sodium'], failed: [] })).toBe('Installed Sodium.')
  expect(installedSentence({ installed: ['Sodium', 'Indium', 'Fabric API'], failed: [] })).toBe('Installed Sodium and 2 mods it needs.')
  expect(installedSentence({ installed: ['Iris', 'Sodium'], failed: [] })).toBe('Installed Iris and 1 mod it needs.')
  expect(installedSentence({ installed: ['Sodium'], failed: [{ name: 'Indium', reason: 'no build for fabric on Minecraft 1.21.1' }] })).toBe(
    'Installed Sodium. Indium was not installed: no build for fabric on Minecraft 1.21.1.',
  )
  expect(installedSentence({ installed: [], failed: [{ name: 'Sodium', reason: 'it comes from the pack, and the sync manages it' }] })).toBe(
    'Sodium was not installed: it comes from the pack, and the sync manages it.',
  )
  expect(installedSentence({ installed: ['Sodium'], failed: [{ name: 'a.jar', reason: 'The old file a.jar could not be removed: remove it yourself.' }] })).toBe(
    'Installed Sodium. The old file a.jar could not be removed: remove it yourself.',
  )
})
test('browser texts', () => {
  expect(noResultsText({ kind: 'neoforge', version: '21.1.0' }, '1.21.1')).toBe('No mods found for NeoForge on Minecraft 1.21.1.')
  expect(noResultsText({ kind: 'fabric', version: null }, '1.20.1')).toBe('No mods found for Fabric on Minecraft 1.20.1.')
  expect(downloadsText(254364989)).toBe('254,364,989 downloads')
  expect(downloadsText(1)).toBe('1 download')
  expect(browseNote(inst())).toBe('Mods need a mod loader, and this instance has none.')
  expect(browseNote(homestead())).toBeNull()
})
