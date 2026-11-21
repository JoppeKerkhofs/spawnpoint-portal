import { expect, test } from "vitest";
import type { Instance, Source } from "../api";
import type { Playing } from "./operation";
import { canCreate, loaderChoice, versionChoices, dropInstance, instanceList, instancesState, instancesSummary, kindOf, kindTone, loaderText, packText, playButton, sourceText } from "./instances";

const inst = (extra: Partial<Instance> = {}): Instance => ({
  id: "i", name: "N", minecraft: "1.20.1", loader: null, source: { kind: "manual" } as Source, managed: [], memory_mb: null,
  last_played: null, manual: [], skipped: [], picture: null, own_picture: false, ...extra,
});
const six = () => Array.from({ length: 6 }, (_, n) => inst({ id: `i${n}` }));

test('summary', () => {
  expect(instancesSummary(null)).toBe('Reading your instances…')
  expect(instancesSummary({ instances: [], unreadable: [] })).toBe('Nothing here yet')
  expect(instancesSummary({ instances: six(), unreadable: [{ folder: 'old-test' }] })).toBe('6 instances · 1 couldn’t be read')
  expect(instancesSummary({ instances: six(), unreadable: [] })).toBe('6 instances')
  expect(instancesSummary({ instances: [inst()], unreadable: [] })).toBe('1 instance')
  expect(instancesSummary({ instances: [], unreadable: [{ folder: 'x' }] })).toBe('0 instances · 1 couldn’t be read')
})
test('state', () => {
  expect(instancesState(null)).toBe('loading'); expect(instancesState({ instances: [], unreadable: [] })).toBe('empty')
  expect(instancesState({ instances: [], unreadable: [{ folder: 'x' }] })).toBe('filled'); expect(instancesState({ instances: six(), unreadable: [] })).toBe('filled')
})
test('where an instance comes from', () => {
  const panels = [{ id: 'panel-joppek-be', address: 'https://panel.joppek.be', name: 'Joppe’s Spawnpoint' }]
  const home = inst({ name: 'Homestead', source: { kind: 'server', panel: 'panel-joppek-be', server: 'homestead', pack: { catalogue: 'modrinth', project: 'x', version: 'y', version_name: '1.3.7' } } })
  expect(sourceText(home, panels)).toBe('Homestead on Joppe’s Spawnpoint · Modrinth pack 1.3.7')
  expect(sourceText({ ...home, source: { kind: 'server', panel: 'gone', server: 'h', pack: null } }, panels)).toBe('Homestead on gone')
  expect(sourceText(inst({ source: { kind: 'pack', pack: { catalogue: 'ftb', project: '1', version: '2', version_name: null } } }), panels)).toBe('FTB pack')
  expect(sourceText(inst({ source: { kind: 'manual' } }), panels)).toBe('Made by you')
  expect(kindOf(inst({ source: { kind: 'manual' } }))).toBe('By hand')
  expect(kindOf(home)).toBe('Server'); expect(kindTone(home)).toBe('own'); expect(kindTone(inst())).toBe('hand')
  expect(loaderText({ kind: 'neoforge', version: '21.1.77' })).toBe('NeoForge 21.1.77'); expect(loaderText(null)).toBe('No mod loader')
  expect(packText({ catalogue: 'modrinth', project: 'x', version: 'y', version_name: '1.3.7' })).toBe('Modrinth pack 1.3.7')
})
test('create needs a name and versions', () => {
  expect(canCreate('  ', ['26.3'])).toBe(false); expect(canCreate('My new world', null)).toBe(false); expect(canCreate('My new world', ['26.3'])).toBe(true)
})
test('snapshots are listed only when asked for', () => {
  const all = [{ id: '26.4-snapshot-10', snapshot: true }, { id: '26.3', snapshot: false }]
  expect(versionChoices(all, false)).toEqual(['26.3']); expect(versionChoices(all, true)).toEqual(['26.4-snapshot-10', '26.3']); expect(versionChoices(null, true)).toBe(null)
})
test('the loader choice names a version only when one is picked', () => {
  expect(loaderChoice('none', '1')).toBe(null); expect(loaderChoice('fabric', '')).toBe('fabric'); expect(loaderChoice('forge', '47.4.28')).toBe('forge:47.4.28')
})
test('play button', () => {
  const i = inst({ name: 'Homestead' })
  const idle = { phase: 'idle' } as const
  const prep = { phase: 'preparing', name: 'Homestead', step: 1, steps: 6, stage: 'check', done: 0, total: 0 } as const
  const on = (id: string): Playing => ({ instance: id, name: 'x', since: new Date(), host: null, memoryMb: 4096, serverKey: null })
  expect(playButton(i, idle, null, [])).toEqual({ label: 'Play', aria: 'Play Homestead', disabled: false })
  expect(playButton(i, idle, null, [on('i')])).toEqual({ label: 'Running', aria: 'Homestead is running', disabled: true })
  expect(playButton(i, prep, 'i', [])).toEqual({ label: 'Preparing…', aria: 'Preparing… Homestead', disabled: true })
  expect(playButton(i, prep, 'other', [])).toEqual({ label: 'Play', aria: 'Play Homestead', disabled: true })
  expect(playButton(i, { phase: 'starting', name: 'x', key: 'i' }, 'i', [])).toMatchObject({ label: 'Starting…', disabled: true })
  expect(playButton(i, idle, null, [on('other')])).toMatchObject({ label: 'Play', disabled: false })
})
test('a deleted instance leaves the shared list at once', () => {
  instanceList.value = { instances: [inst({ id: 'a' }), inst({ id: 'b' })], unreadable: [] }
  dropInstance('a')
  expect(instanceList.value.instances.map((i) => i.id)).toEqual(['b'])
  instanceList.value = null
  dropInstance('b')
  expect(instanceList.value).toBeNull()
})
