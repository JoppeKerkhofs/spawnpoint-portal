# What the launcher still needs

**Goal:** the things a player expects from a Minecraft launcher that Portal does not do yet. Each piece below stands on its own, gets a branch and a pull request of its own, and ends with something that runs.

**Where it stands:** piece 1 is written. The rest is not started, and none of it has a design yet: each piece below says what is wanted and where the code is, not how to build it. Read the code it names first, and settle the open questions with the owner before writing.

## Status

Updated 25 November 2026. Change this table in the pull request that changes a piece.

| Piece | State | Pull request | Still to check |
|---|---|---|---|
| 1. Game log and crash view | Open | #24 | By hand: the log card in each look, a game that crashes, a Java that does not start, Copy |
| 2. Install a pack from the window | Not started | | |
| 3. Instance housekeeping | Not started | | |
| 4. Java arguments, the player's and the server's | Not started | | Needs a panel version first |
| 5. Shaders and resource packs in the browser | Not started | | |
| 6. Config files in a server sync | Not started | | |
| 7. Total play time | Not started | | |
| 8. Start in a singleplayer world | Not started | | |
| 9. Screenshots | Not started | | |
| 10. World backup before a pack update | Not started | | |
| 11. Export and import an instance | Not started | | |
| 12. A second Microsoft account | Not started | | |
| 13. Changing the skin | Not started | | |

Pieces 2 to 6 are the ones a player misses; 7 to 13 are nice to have. Do them in this order unless the owner says otherwise.

**Not wanted:** starting or stopping a server from the launcher. The owner decided this on 25 November 2026; the design's non-goal "managing servers from the launcher" stands.

## Rules for every piece

- The design (`docs/superpowers/specs/2026-11-19-spawnpoint-portal-design.md`) still holds: launcher logic is hand-written in `crates/core` and does not depend on Tauri; ask the owner before adding a crate that does launcher work; a sync never deletes or overwrites a file the player added; no token leaves the machine or reaches a log or the window.
- The shape of a feature: logic in `crates/core/src`, a thin command in `src-tauri/src/instances.rs` registered in `src-tauri/src/lib.rs`, its call in `src/api.ts`, pure decisions in `src/logic/*.ts` with a test beside them, and the screen in `src/views` or `src/components`.
- Whatever changes an instance's files refuses while it runs (`refuse_if_running`) and takes the instance's claim (`claim`), as `instance_delete` and `mods_install` do.
- New fields in `instance.json` get `#[serde(default)]`, so an older file still reads.
- The command line (`crates/core/src/main.rs`) can do what the window can; add the command there too and to the README.
- Checks: `cargo test --workspace`, `npm test`, `npx vue-tsc --noEmit`. Then try it in the window, and write in the status table what was not tried.

## 1. Game log and crash view

Written. The game's output goes to `logs/portal-output.log` in the game folder (`gamelog::capture`, used by `launch::command` and `horizon::launch`), new at every start. For a version with a Mojang log config only the error output is kept, because its standard output repeats `latest.log` as XML. `gamelog::read` returns the newest crash report, that output and `latest.log`, each cut to its last 256 KB, with anything that looks like an access token replaced; a crash report or `latest.log` older than the last start is left out. The window shows it in `GameLog.vue` behind the Log button of an instance, and the status bar offers it when a game exits with anything but code 0 (`stopped` and `showLog` in `src/logic/operation.ts`).

Not done: following the log live while the game runs (Refresh reads it again), and `hs_err_pid*.log` files, which hold the full command line with the token and so need more care than the redaction gives.

## 2. Install a pack from the window

The core installs an FTB or Modrinth pack without a server; only the command line offers it (`pack_install` and `pack update` in `main.rs`, `pack::resolve` and `pack::install` in `crates/core/src/pack.rs`). CurseForge packs need a panel's key and stay tied to a server.

Wanted: on the Instances page, a way to search packs, pick a version and install it as an instance, with the progress in the status bar like a join; and on a pack instance, a way to move to another version of its pack.

Where: `ModBrowser.vue` and `mods::search` are the model for the search (Modrinth's `project_type:modpack` facet). FTB's public API has a search and a pack listing; check its current documentation. `Instance.manual` already carries the files to fetch by hand.

Open: Modrinth only at first, or FTB as well; whether the pack's icon becomes the instance's picture.

## 3. Instance housekeeping

`crates/core/src/instance.rs` has create and delete only.

Wanted: rename; duplicate (a copy of the whole instance folder under a new id); change the Minecraft version or the loader and its version of an instance the player made; repair (check the game files and the managed files again and fetch what is wrong).

Where: `instance::update` for the changes, `instance::slug` for ids, `gamefiles::install` already checks every file against its hash, `sync::plan` knows the managed files.

Open: whether the id (the folder name) follows a rename, or only the name does (the second is far less work and breaks nothing); whether version and loader may change on a server's or a pack's instance (they should not: the next sync decides them); what happens to mods that do not fit the new version.

## 5. Shaders and resource packs in the browser

`mods::search` is fixed to `project_type:mod` and `mods::install` puts files in the mods folder (`Instance::mods_place`).

Wanted: the same browser for resource packs (`resourcepacks/`) and shaders (`shaderpacks/`), with the installed ones listed and removable.

Open: whether shaders are offered only when a shader mod (Iris, or OptiFine by hand) is in the instance; whether updates are checked as for mods.

## 6. Config files in a server sync

A sync covers jars only (the design: "Jars only, no `config/`"). A server whose mods need matching configs still needs hand work.

Wanted: a server's instance also gets the config files the server's owner wants clients to have.

This needs the panel first: `GET /servers/{server}/client-mods` lists jars only, so a new route or new fields are needed, built in the spawnpoint repository and described in its `openapi.json`. On the launcher side `sync::plan` already works on managed paths with hashes, not only on jars.

Open, and decisive: which files the panel sends (a server's `config/` holds server-only files and can hold secrets, so the owner has to choose them, never the whole folder); what happens to a config the player changed (the design's rule keeps a changed managed file, which for configs means a server change never arrives).

## 7. Total play time

`Instance.last_played` holds the last session only (`instance::record_played`).

Wanted: the total time played per instance, shown where "last played" is (`lastPlayedText` in `src/logic/format.ts`).

## 8. Start in a singleplayer world

Wanted: the worlds of an instance listed (the folders of `saves/`), each with a button that starts the game in it.

Where: Minecraft 1.20 and newer take `--quickPlaySingleplayer <world folder>`; `launch::arguments_on` already does this for `is_quick_play_multiplayer`, and the feature here is `is_quick_play_singleplayer`. Older versions have no such argument.

Open: hide the button on older versions, or show the list without it.

## 9. Screenshots

Wanted: an instance's `screenshots/` shown as a grid, with open, show in folder and delete.

Open: how the window reads the pictures (Tauri's asset protocol needs the folder allowed in `src-tauri/capabilities/default.json`; thumbnails sent as data are the alternative).

## 10. World backup before a pack update

Wanted: before a sync or a pack update changes an instance whose pack version differs, its `saves/` are copied, so a world broken by the update can be put back.

Open: where the copies go and how many are kept; whether it is asked every time or set once; zip or plain copy (`zip` is already a dependency).

## 11. Export and import an instance

Wanted: an instance written as a `.mrpack` to give to a friend, and a `.mrpack` file from disk installed as an instance.

Where: `pack::modrinth::read_mrpack` already reads the format, so import is the small half. Export needs each mod's download address, which `mods::installed` finds on Modrinth by hash; a jar Modrinth does not know goes into `overrides/`.

Open: which folders an export includes besides mods (configs, resource packs; never saves or logs).

## 12. A second Microsoft account

One account is kept (`auth.rs`, its refresh token under one name in `secrets.rs`, `Settings.account_name`). The design lists more than one account as a non-goal.

Wanted: several accounts signed in, one of them active, switched on the Account page.

Open: whether an instance can pin an account.

## 13. Changing the skin

The design lists skins as a non-goal.

Wanted: the current skin shown on the Account page, and a new one uploaded from a file, classic or slim.

Where: the Minecraft services API takes a skin with the access token `auth::current_profile` already gets; check its current documentation. The token must stay in the core.
