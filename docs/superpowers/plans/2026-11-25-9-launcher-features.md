# What the launcher still needs

**Goal:** the things a player expects from a Minecraft launcher that Portal does not do yet. Each piece below stands on its own, gets a branch and a pull request of its own, and ends with something that runs.

**Where it stands:** pieces 1 to 3 are written. The rest is not started, and none of it has a design yet: each piece below says what is wanted and where the code is, not how to build it. Read the code it names first, and settle the open questions with the owner before writing.

## Status

Updated 26 November 2026. Change this table in the pull request that changes a piece.

| Piece | State | Pull request | Still to check |
|---|---|---|---|
| 1. Game log and crash view | Open | #24 | By hand: the log card in each look, a game that crashes, a Java that does not start, Copy |
| 2. Install a pack from the window | Open | #25 | By hand, all of the window: the pack search, the version list, an install with its progress, Change version on the instance page. FTB is not in it |
| 3. Instance housekeeping | Open | #26 | By hand, all of the window: the Manage card (rename, duplicate, change version), Check files again on a pack |
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

Written for Modrinth. **Install a pack** on the Instances page opens `PackBrowser.vue`: a search (`pack::modrinth::search`, the mod search with the `project_type:modpack` facet), and per pack a name, a version (`pack::modrinth::versions`, without the versions for Quilt alone) and Install. The instance page of a Modrinth pack installed without a server has a **Pack version** card that moves it to another version. Both go through `pack::install_new` and `pack::update`, which the command line uses too, with the progress in the status bar. The pack's icon becomes the instance's picture, as a server's does; the player's own picture still wins.

Chosen without the owner, and easy to turn: Modrinth only, and the icon as the picture.

Not done: FTB in the window. Its public API has a search and a pack listing; check its current documentation. An FTB pack still installs from the command line, and its instance has no Pack version card.

## 3. Instance housekeeping

Written. An instance page has **Manage**, a card with three forms; the command line has `portal instance rename`, `duplicate` and `set-version`.

- **Rename** (`instance::rename`) changes the name only. The id, and so the folder, stays: nothing that points at the instance breaks.
- **Duplicate** (`instance::duplicate`) copies the whole game folder under a new id. A copy of a pack's instance stays that pack's. A copy of a server's instance becomes the player's own, with every mod as an own mod: a sync finds a server's instance by its source, so a server has one.
- **Change version** (`instance::set_version`) sets the Minecraft version, the loader and its version, for an instance the player made and not for Horizon. A server's or a pack's is refused: the next sync decides it. Mods are left as they are, and the form says one made for another version can stop the game. The fields are `VersionFields.vue`, which the New instance form uses too.
- **Repair** needed almost nothing new. Every Play already checks each game file against its hash (`download::fetch`), and a join does the same for a server's files. What was missing is a pack without a server: its Pack version card now says **Check files again** when the version is the current one, which installs that version over the instance and brings back what is gone.

Chosen without the owner: the three answers above (name only, no version change on a server's or pack's instance, mods left alone).

Not done: a pack file the player changed is not put back by Check files again. The design keeps a changed managed file as the player's.

## 4. Java arguments, the player's and the server's

Only memory can be set (`Instance.memory_mb`, `-Xmx` in `launch::arguments_on`).

Wanted, in two halves:

- **The player's.** A text field on an instance for extra JVM arguments.
- **The server's.** A server's owner sets Java arguments for the players' game in the panel. The launcher reads them and applies them to that server's instance on every sync, without the player doing anything. The owner extended the piece with this on 25 November 2026.

The two are kept apart in `instance.json` (a field for each), as pack mods and own mods are: a sync replaces the server's arguments and never touches the player's. The order at the start is `-Xmx`, then the server's, then the player's, so the player's own choice wins where both name the same flag.

**The panel first** (the spawnpoint repository, a version of its own, before the launcher half that reads it):

- A new setting beside the existing `jvm_opts`, which is for the server's own Java and must not be reused: a server tuned with `-Xms` and garbage collector flags for a 12 GB server process is not what a player's game wants. The existing one is validated in `panel/app/Concerns/ServerValidationRules.php`, stored by `panel/app/Actions/Servers/UpdateServer.php` and edited in `panel/resources/js/components/ServerFields.vue` ("Extra JVM flags"); the new one follows it, with a label that says it is for players who join with Portal.
- `GET /servers` and `GET /servers/{server}` answer it beside `game_version` (`panel/app/Http/Controllers/Api/V1/ServerController.php`), null or absent when nothing is set, and `panel/resources/api/openapi.json` describes it.

**The launcher:**

- `panel::Server` gets the field with `#[serde(default)]`, so an older panel still reads. The sync (`join::sync_numbered`) stores it on the instance; a server that stops sending it clears it.
- The instance page shows the server's arguments as read-only, marked as coming from the server, above the player's own field.
- The launcher checks the server's arguments itself, whatever the panel checked: the panel is another machine, and these become part of a command line on the player's PC.

Decided by the owner on 25 November 2026:

- **Which arguments a server may send.** A server's owner already decides which mods run on a player's machine, so this adds no new trust, but arguments can do things a mod list does not show. The launcher refuses anything that loads code or runs a command (`-javaagent`, `-agentlib`, `-agentpath`, `-cp`, `-classpath`, `-jar`, `-XX:OnError`, `-XX:OnOutOfMemoryError`, an `@file`), and `-Xmx` and `-Xms`, because memory is the player's choice and their machine's limit. A refused argument is left out and named on the instance page, and the game still starts.
- **Horizon** instances have no Java (they hide memory already, `isHorizon` in `InstanceView.vue`) and get neither half.

Open:

- How the text becomes arguments: split on spaces, or with quotes for a value that holds a space.
- Whether a wrong argument of the player's own is caught before the start or left to the log of piece 1 (the second is simpler, and piece 1 makes it visible).
- Whether the player may switch the server's arguments off for their instance.

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
