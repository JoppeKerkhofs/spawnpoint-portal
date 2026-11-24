# Spawnpoint Portal: design

19 November 2026. Supersedes the open questions of the
[handover doc](https://claude.ai/artifact/NF175MZH2NHy1Rtip8FumJ).

Lines marked **(call)** were decided while writing this, not by the owner; they are the ones to
check.

## Intent

A Minecraft: Java Edition launcher for the owner and a few friends, written from scratch.
Existing launchers feel slow or unfriendly, the owner enjoys building one, and a launcher of our
own can do what no other can: read a [Spawnpoint](../../../../spawnpoint) server's version,
loader and mods from the panel and have everything ready before the player joins.

## Goals

- Instances: each has a Minecraft version, an optional loader, and its own game folder with mods.
- Joining a Spawnpoint server creates or updates the instance for it, then starts the game
  connected to that server.
- A modpack server gives the client side of its pack, at the version the server runs. All three
  catalogues the panel offers: FTB, Modrinth, CurseForge.
- A plain modded server gives a list of mods, sent by the panel, that the launcher downloads.
- Players can add their own mods to any instance; a sync never removes them.
- Several panels can be linked at once; their servers appear in one list.
- Sign-in with a real Microsoft account.

## Non-goals

- macOS and Linux. Windows 10/11 x64 only for now; nothing should make them impossible later.
- Code signing, public distribution, an auto-updater.
- Managing servers from the launcher (power, console, files). List and join only **(call)**.
- Minecraft older than 1.13 for now. Versions back to 1.6.4 are wanted later, as a piece of
  their own (the older argument format, asset layouts, Java runtimes and Forge installer);
  nothing before 1.6.4.
- Skins, Bedrock, Realms, more than one Microsoft account **(call)**.
- Finding a Java already on the machine. The launcher always fetches Mojang's own runtime
  **(call)**: one path instead of two.

## Anti-goals

- A sync that deletes or overwrites a mod the player added.
- A Microsoft or panel token that leaves the player's machine, or a CurseForge key that reaches it.
- A slow start: the window opens before any network call finishes.
- Launcher logic that depends on Tauri, so that it cannot be run or tested from a terminal.

## Constraints

- Rust, Tauri v2, Vue 3 with TypeScript; Tailwind v4 as in the panel **(call)**.
- All launcher logic is hand-written. Crates only for plumbing: `reqwest`, `tokio`, `serde`,
  `serde_json`, `sha1`, `sha2`, `zip`, and `keyring` for the Windows Credential Manager **(call)**.
  Ask the owner before adding a crate that does launcher work.
- The launcher talks only to a panel's public API (`/api/v1`), never to a node.
- Rust is not installed on the owner's machine yet: `rustup` with the MSVC toolchain and the
  Visual Studio C++ build tools come first.
- Library and API details below come from documentation read in November 2026. Check the current
  documentation of each before building on it.

## Approaches

**How much is hand-written.** Considered: (1) everything by hand, crates for plumbing only;
(2) the same, with `minecraft-msa-auth` for the login chain; (3) crates wherever one exists.
The owner picked (1): building it is the point.

**Which mods a client gets.** Considered: (1) the launcher mirrors the server's `mods/` folder
and filters by Modrinth's side flags; (2) a hand-kept client list per server in the panel;
(3) packs only. The owner picked a mix: a pack server installs the pack's client side, and for a
plain modded server the panel sends the list. A pack server's own changes are applied on top of
its pack, so a client always has every mod the server has. The panel builds its list from the
server's `mods` folder, so nothing has to be kept by hand.

**Order.** Core first from the command line, the window later, Microsoft login last because it
waits on Mojang. The owner confirmed this over having a window early.

## The parts

```
crates/core/     # library with all launcher logic, plus a small command-line binary
src-tauri/       # Tauri commands and events around core
src/             # Vue
```

One library crate with a `main.rs` beside it **(call)**; no shared crate with the platform, whose
server definition lives in Laravel. The contract between the two is the panel's `openapi.json`.

### On disk

`%APPDATA%\SpawnpointPortal\` **(call)**:

| Path | Holds |
| --- | --- |
| `meta/versions`, `meta/libraries`, `meta/assets` | Game files, shared by every instance |
| `java/<component>/` | Mojang's Java runtimes |
| `instances/<id>/instance.json` | Name, Minecraft version, loader and its version, where it came from, the managed files |
| `instances/<id>/game/` | The game folder: `mods/`, `config/`, saves, options |
| `panels.json` | Linked panels: id, address, name |

Tokens (Microsoft refresh token, one token per panel) go in the Windows Credential Manager, not
in a file.

### Core, piece by piece

1. **Game files.** Read Mojang's version manifest, then one version's JSON; download the client
   jar, the libraries its rules allow on Windows, the asset index and the assets. Every file is
   checked against its SHA-1, and a file that is already right is not fetched again. Downloads
   run several at a time and report progress through a channel the caller reads.
2. **Java.** The version JSON names a runtime component; fetch it from Mojang's runtime manifest
   for `windows-x64`.
3. **Launch.** Build the classpath and the JVM and game arguments from the version JSON, with its
   rules and placeholders, extract natives where the version still needs it, and start Java with
   the instance's game folder. Until login exists, an offline profile.
4. **Loaders.** Fabric: the profile JSON from Fabric's meta service, laid over the vanilla one.
   NeoForge and Forge: download the installer, read its install profile, fetch its libraries and
   run its processors with our Java.
5. **Instances.** Create, list, delete, add and remove a mod file. `instance.json` lists the
   files a sync put there with their hashes; a sync touches only those.
6. **Pack installers.** One per catalogue, each turning a pack and version into a loader, a
   Minecraft version and a set of managed files:
   - FTB: the public API's answer for a pack version
     (`api.feed-the-beast.com/v1/modpacks/public/modpack/<pack>/<version>`): its `targets` name
     the Minecraft version and the loader, and each of its `files` has `path`, `name`, `url`,
     `sha1`, `size`, `clientonly` and `serveronly`. Server-only files are skipped. In two current
     packs checked on 19 November 2026 every file had a `url`; one without is listed for the
     player to fetch, like a closed CurseForge mod.
   - Modrinth: the `.mrpack` archive, its index (skipping files unsupported on the client) and
     its `overrides` and `client-overrides` folders.
   - CurseForge: the pack archive's `manifest.json` and overrides. The archive's address and the
     file addresses come from the panel, which holds the key, so this installer only works for
     an instance that belongs to a panel's server. A mod whose author forbids outside downloads is listed with
     its page address and a button that opens the instance's `mods` folder **(call)**.
7. **Panel client.** Pair with a panel by code, keep several panels, list their servers, fetch
   what an instance needs, and sync.
8. **Microsoft login.** Device code flow against the `consumers` tenant with scope
   `XboxLive.signin offline_access`, then Xbox Live, XSTS, `login_with_xbox`, the entitlement
   check and the profile. The refresh token is kept; the rest is fetched again when needed. The
   client ID is a constant in the code: it is a public client and has no secret.

### Joining a server

1. The launcher asks the panel what the server runs.
2. It finds the instance for that panel and server, or makes one.
3. Pack server: run the pack installer for the reported pack and version, then apply the
   server's own changes on top, so the client always has every mod the server has, also the
   ones added by hand. Plain modded server: install the loader and download the listed mods.
   Vanilla or Paper-like: just the version.
4. Managed files that are no longer wanted are removed; everything else in the folder stays.
5. The game starts and connects to the server's `hostname` on port 25565: `--quickPlayMultiplayer`
   from Minecraft 1.20, `--server` and `--port` before it.

### What the panel offers a launcher

Built in the panel's version 1.7.0 (commit `7d559e8` on `feature/JKER_1.7.0_modpack_update_check`,
not merged when this was written). All under `/api/v1` and described in the panel's
`openapi.json`, which is the reference; this is a summary.

**Access.** Whoever may see a server may join it: the launcher routes need `can:view,server` and
the token ability `servers:join`, which the read-only preset includes. They do not need the
`files` share. A launcher's token therefore wants `servers:read` and `servers:join`; when one is
missing the launcher says so and names the panel page where it is granted.

**What a server runs.** `GET /servers` and `GET /servers/{server}` answer, beside what they
already did:

```json
"game_version": "1.21.1",
"modpack": { "catalogue": "ftb", "project": "126", "version": "100596", "version_name": "1.23.0" },
"loader": { "name": "neoforge", "version": "21.1.265" }
```

- `game_version` is the real Minecraft version, also where `mc_version` says `LATEST` or a pack
  decides it. The launcher uses this one, never `mc_version`.
- `modpack` is null without a pack. `catalogue` is `ftb`, `modrinth` or `curseforge`. `version`
  is what is installed, read from the records the Minecraft image leaves in the server's folder,
  and null until the pack has been installed once. For Modrinth, `project` may be a pasted
  `https://modrinth.com/modpack/...` link instead of an id.
- `loader` is null for a server without one. Its `version` may be null; the launcher then takes
  the newest for that Minecraft version.
- These are at most a minute old.

**`GET /servers/{server}/client-mods`.** What the game needs in its `mods` folder:

```json
{ "mods": [ { "file": "x.jar", "sha512": "…", "size": 123, "url": "https://cdn.modrinth.com/…" } ],
  "removed": ["a-pack-mod.jar"] }
```

For a server without a pack, every jar. For a pack server, only the jars added on top of the
pack, and in `removed` the pack's jars that were taken out. Mods Modrinth marks unsupported on
the client are left out. `url` is null for a jar Modrinth does not know. Answers 409 while a pack
server's pack is not installed yet.

**`GET /servers/{server}/client-mods/{file}`.** One jar from the server's `mods` folder, for the
ones without a `url`.

**`GET /servers/{server}/client-pack`** (CurseForge servers only, 404 otherwise, 409 without a
key in the panel): `{ "project", "file", "archive", "page" }`. `archive` is the pack's zip, or
null when its author forbids outside downloads; `page` is where to fetch it by hand then.

**`POST /servers/{server}/client-pack/files`** with `{ "files": [<file id>, …] }`, at most 2000:
`{ "files": [ { "project", "file", "name", "url", "page" } ] }`. The launcher reads the file ids
from the `manifest.json` inside the archive. `url` is null where downloads are forbidden, and
`page` then names the page of that file.

**Pairing** (`POST /pair`, `POST /pair/token`) was there already.

### Rules of a sync

- On every join, not only the first: with automatic pack updates a server changes on its own.
- A pack server: install the pack at `modpack.version`, then download what `client-mods` lists
  and delete what it names in `removed`.
- Jars only, no `config/` **(call)**.
- A managed file the player changed (its hash no longer matches `instance.json`) is kept, never
  overwritten **(call)**.

### The window

A server list across all linked panels, the instances, one instance with its mods, the panels,
the account, and a progress view fed by Tauri events. Styling follows the panel's look by
copying what is needed, not by sharing a package **(call)**.

## Order of work

One plan per piece; each ends with something that runs.

0. Azure app and Mojang review. Done by the owner: submitted 19 November 2026, approved 24 November 2026.
1. Core from the command line: game files, Java, launch offline.
2. Loaders: Fabric, then NeoForge and Forge.
3. Instances and pack installers: FTB first (the owner's live server), then Modrinth.
4. The panel client: pairing, several panels, sync, join, and the CurseForge installer, which
   needs the panel.
5. The Tauri and Vue window.
6. Microsoft login, once Mojang has approved.
7. Minecraft 1.6.4 to 1.12.2.

An online-mode server can only be joined after piece 6.

## Checks

Unit tests with saved JSON for the parts that parse and decide: library rules, argument
building, the three pack formats, which managed files a sync removes. Downloading and launching
are checked by running the command-line binary. The panel client is checked against the owner's
running panel.

## Left to the builder

- How many downloads run at once, retry behaviour, and the exact layout of `instance.json`.
- The exact spelling of `loader.name` for each loader, as the panel sends it: check against a
  running server of each kind before matching on it.
- What the window looks like, screen by screen: decided with the owner when piece 5 starts.
