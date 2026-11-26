# What the launcher still needs

**Goal:** the things a player expects from a Minecraft launcher that Portal does not do yet. Each piece below stands on its own, gets a branch and a pull request of its own, and ends with something that runs.

**Where it stands:** pieces 1 to 7 and 9 to 11 are written (of pieces 4 and 6 the launcher side; the panel sides are pull requests 16 and 17 of the spawnpoint repository, versions 1.9.7 and 1.9.8). The rest is not started, and none of it has a design yet: each piece below says what is wanted and where the code is, not how to build it. Read the code it names first, and settle the open questions with the owner before writing.

## Status

Updated 26 November 2026. Change this table in the pull request that changes a piece.

| Piece | State | Pull request | Still to check |
|---|---|---|---|
| 1. Game log and crash view | Open | #24 | By hand: the log card in each look, a game that crashes, a Java that does not start, Copy |
| 2. Install a pack from the window | Open | #25 | The owner tried it on 26 November 2026 and it works. FTB is not in it |
| 3. Instance housekeeping | Open | #26 | The owner tried it on 26 November 2026, the reworked page too |
| 4. Java arguments, the player's and the server's | Open | #27 | By hand: the Java arguments card, a start with an argument, a wrong argument in the Log, a sync with a panel that sends arguments (needs the panel's version with the setting) |
| 5. Shaders and resource packs in the browser | Open | #28 | The owner tried it on 26 November 2026: a resource pack and a shader both show in the game |
| 6. Config files in a server sync | Open | #29 | By hand, all of it: ticking a file and a folder in the panel (needs its version 1.9.8, node too), a sync that brings them, a changed copy that is replaced and kept as `.portal-backup`, unticking |
| 7. Total play time | Open | #30 | By hand: play twice and read the line at the bottom of the instance's page |
| 8. Start in a singleplayer world | Left out | | The owner left it out on 26 November 2026: not something anyone would really use |
| 9. Screenshots | Open | #31 | By hand: the Screenshots button on an instance's page |
| 10. World backup before a pack update | Open | #32 | By hand: a world in a pack's instance, another version of the pack, the copy in `world-backups` |
| 11. Export and import an instance | Open | #33 | By hand: Export on an instance's page, Import a file on the Instances page, the file opened in another launcher |
| 12. A second Microsoft account | Left out | | The owner left it out on 26 November 2026 |
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

Written. An instance page has a pencil that makes its title a field (Enter or leaving it saves, Escape does not), a copy button that duplicates at once as "<name> copy" and opens the copy, and for an instance the player made a **Change version** button with its form. The owner asked for the two icon buttons on 26 November 2026, in place of a Manage card. The command line has `portal instance rename`, `duplicate` and `set-version`.

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

Written, the launcher's side. `Instance` has `java_args` (the player's text), `server_java_args` and `refused_java_args` (what the last sync got, split by `launch::server_arg_allowed`); `Instance::java_arguments` gives the start its list and checks the server's again. The panel's field is `client_jvm_opts`. The page has a **Java arguments** card at its end; the command line has `portal instance java-args`.

The three open questions, answered without the owner:

- The text is split on spaces, and double quotes keep a value with a space together (`launch::split_args`).
- A wrong argument of the player's own is not caught: Java says which one in the Log of piece 1, and the card says to look there.
- No switch to turn the server's arguments off. The player's come last, so the player can already set a flag the other way.

Beyond the owner's list the launcher also refuses, from a server: a word without a dash (Java would take it for the class to start), the other spellings of the class and module path, `-Xbootclasspath`, `--patch-module`, the heap and RAM `-XX` flags, `-XX:Flags` and `-XX:VMOptionsFile` (a file of more flags), and `-Djava.class.path`, `-Djava.system.class.loader`, `-Dcom.sun.management` (remote JMX) and `-Dlog4j`. It is a list of what is refused, so it cannot be proven complete; a list of what is allowed would be, at the cost of a server owner's freedom.

## 5. Shaders and resource packs in the browser

Written. The card that lists an instance's mods has a switch: **Mods**, **Resource packs**, **Shaders**. Each has the same list (pack or own, Remove), search, **Browse** on Modrinth and **Add a zip file**; the folder button opens the folder of the kind shown. In the core it is one `instance::Kind` passed to what already existed (`instance::files`, `add_file`, `remove_file`, `mods::search_of`, `installed_of`, `install_of`): a kind is a folder (`resourcepacks`, `shaderpacks`), a file ending (`.zip`) and a Modrinth project type. The command line takes `--kind` on `instance mods`, `add-mod` and `remove-mod`.

What differs from a mod: a resource pack or a shader is tied to the Minecraft version alone, so an instance without a mod loader can have them; what it needs is not installed with it (that would be a mod, in the wrong folder); and no newer build is looked for. Horizon has neither.

The two open questions, answered without the owner: shaders are always offered, with a line that says they need a shader mod such as Iris, or OptiFine; updates are not checked.

Not done: a resource pack that is a folder and not a zip is not listed.

## 6. Config files in a server sync

Written. A server's owner ticks files and folders on the server's Files page in the panel; a sync puts each of them at the same path in the instance. A ticked folder means everything in it, also what is added later.

Decided by the owner on 26 November 2026:

- The owner of a server ticks files and folders. A list of paths typed in a field was not enough: a server rarely needs one file, more often a handful of configs or whole folders such as `kubejs/`.
- The server's copy wins. A copy the player has that the launcher did not put there is kept beside it as `<name>.portal-backup`, one backup per file, replaced by the next. Many mods rewrite their config on every start, so the jars' rule (a changed file is left alone) would let a server change arrive almost never.

How: the panel's `GET /servers/{server}/client-mods` also answers `files` (path, SHA-512, size) and `GET /servers/{server}/client-files/{path}` passes one on. `join::build` turns each into a `sync::Wanted` with `force` set; `sync::plan` then replaces what is there whatever it is, and `sync::apply` copies it to the backup first unless it is what the launcher put itself. A file that is no longer ticked is deleted by the rule that was there: only while it is still what the launcher put.

Both sides hold a server to four folders, below the folder itself: `config/`, `defaultconfigs/`, `kubejs/`, `scripts/` (`join::CLIENT_ROOTS`, the panel's `Server::CLIENT_ROOTS`). So a server cannot put a jar, touch a world or the options file, and never sends a whole `config/`. The launcher skips anything else a panel lists.

Not done: nothing in the window says which files came from the server or that a backup was made. A server without mods has no such list (the panel answers 404 for it, as for its mods).

## 7. Total play time

Written. `Instance.played_seconds` holds the total, and `instance::record_played` adds each session to it. The line at the bottom of an instance's page reads "Last played Homestead today, for 1 h 12 min. 14 h in total." (`lastPlayedText` in `src/logic/format.ts`); the total is left out while it is no more than the last session.

An instance from before this starts its total with the one session it remembers. A duplicate starts at nothing. The command line does not show it.

## 8. Start in a singleplayer world

Left out by the owner on 26 November 2026. What was wanted, for whoever takes it up again:

Wanted: the worlds of an instance listed (the folders of `saves/`), each with a button that starts the game in it.

Where: Minecraft 1.20 and newer take `--quickPlaySingleplayer <world folder>`; `launch::arguments_on` already does this for `is_quick_play_multiplayer`, and the feature here is `is_quick_play_singleplayer`. Older versions have no such argument.

Open: hide the button on older versions, or show the list without it.

## 9. Screenshots

Written, as the owner cut it down on 26 November 2026: a **Screenshots** button on an instance's page that opens its `screenshots/` folder in the file explorer (`open_screenshots_folder`). A grid in the window, with open and delete, was the first idea and is not wanted.

## 10. World backup before a pack update

Written. The panel backs up a server's world on the server; this is for the worlds on the player's own computer, which nothing else copies.

When a sync or a pack update brings another version of the pack than the instance has, `pack::install` first copies the instance's `saves/` to `world-backups/<the version it had>` in the instance's folder, beside `game` (`instance::back_up_worlds`). The newest three copies are kept. An instance without worlds gets no copy, and a copy that fails stops the update before anything is changed.

The open questions, answered without the owner: a plain copy and not a zip (putting a world back is copying a folder), always and without asking, three copies.

Not done: the window does not say a copy was made, and has no button to put one back. A change of mods without another pack version (a server's owner adding a jar) makes no copy.

## 11. Export and import an instance

Written. An instance's page has **Export**, which asks where to save and writes a `.mrpack`; the Instances page has **Import a file**, which asks for a `.mrpack` and installs it as an instance named after the file. The command line has `portal instance export <id> <file>` and `portal pack import <file> [--name <name>]`.

An export (`pack::modrinth::export`) holds `mods/`, `resourcepacks/`, `shaderpacks/`, `config/`, `defaultconfigs/`, `kubejs/` and `scripts/`, and never worlds, logs, screenshots or the options file. A file directly in one of the first three that Modrinth knows by its hash is listed with its download address; everything else goes into the archive's `overrides/`. The owner's open question (which folders) was answered without the owner.

An import (`pack::import_file`) is an instance of the player's own (`Source::Manual`): a file has no pack to update it from, so there is no pack version to change, and the Minecraft version can be changed by hand.

Tried on the command line with a real pack (Fabulously Optimized 6.5.0 for 1.21.1): 50 files listed, 63 inside, and the import had the same mods and configs as the instance it came from.

Not done: an export has no picture and version "1"; a mod that Modrinth does not know is copied into the file, which its author may not allow to be passed on.

## 12. A second Microsoft account

One account is kept (`auth.rs`, its refresh token under one name in `secrets.rs`, `Settings.account_name`). The design lists more than one account as a non-goal.

Left out by the owner on 26 November 2026. What was wanted, for whoever takes it up again: several accounts signed in, one of them active, switched on the Account page.

Open: whether an instance can pin an account.

## 13. Changing the skin

The design lists skins as a non-goal.

Wanted: the current skin shown on the Account page, and a new one uploaded from a file, classic or slim.

Where: the Minecraft services API takes a skin with the access token `auth::current_profile` already gets; check its current documentation. The token must stay in the core.
