# Instances and Pack Installers (FTB, Modrinth) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `portal pack install ftb <pack> <version>` (or `modrinth <version id>`) makes an instance with the client side of that pack, and `portal launch <instance>` starts it; a player's own mods survive every later sync.

**Architecture:** An instance is a folder with `instance.json` and a `game/` folder. A pack installer only *describes* what a pack wants (`PackInstall`: Minecraft version, loader, wanted files). One sync (`sync::plan`, pure, then `sync::apply`) turns "wanted files" plus "what we put there last time" into downloads and deletions; it is the only code that writes or deletes inside `game/` on a pack's behalf.

**Tech Stack:** as plan 1.

**Spec:** `docs/superpowers/specs/2026-11-19-spawnpoint-portal-design.md` (pieces 5 and 6 of "Core, piece by piece", "Rules of a sync"). Builds on plans 1 and 2.

## Global Constraints

All of plan 1's and 2's, plus:

- A sync never deletes or overwrites a file it did not put there, and never overwrites a managed file the player changed.
- Paths inside `game/` are stored and compared with forward slashes, relative, never with `..`, a drive letter or a leading slash. A wanted file with such a path is an error for the whole sync, before anything is written.
- Server-only pack files are never installed.
- Layout of `instance.json` is this plan's decision (the spec leaves it to the builder); it is given in Task 1.

## Review Focus

- A pack (or a panel, in plan 4) that names a file `../../something` or `C:\Windows\...`: refused, nothing written. Pinned in Task 2.
- The player drops their own `sodium.jar` in `mods/` and a later pack version ships a file with the same name: the player's stays. Pinned in Task 2.
- The player edits a config the pack installed, then the pack updates that config: the player's edit stays. Pinned in Task 2.
- A sync that is interrupted halfway (one download fails): `instance.json` still lists what was really installed, so the next sync neither loses track of files nor deletes the player's. Pinned in Task 3.
- Two instances made with the same name: both exist, with different ids. Pinned in Task 1.

## File Structure

```
crates/core/src/instance.rs          Instance, create/list/load/save/delete, add and remove a mod
crates/core/src/sync.rs              Wanted, plan(), apply()
crates/core/src/pack.rs              PackInstall, PackRef, install_pack()
crates/core/src/pack/ftb.rs          FTB's API answer
crates/core/src/pack/modrinth.rs     .mrpack
crates/core/src/play.rs              + start_instance()
crates/core/src/main.rs              + instance, pack; launch takes an instance
```

---

### Task 1: Instances

**Files:**
- Create: `crates/core/src/instance.rs`

**Interfaces:**
- Consumes: `Paths`, `Loader`, `sha1_file`.
- Produces:

```rust
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub source: Source,
    pub managed: Vec<ManagedFile>,
}
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    Manual,
    Pack { pack: PackRef },
    Server { panel: String, server: String, pack: Option<PackRef> },   // panel id, server slug
}
pub struct PackRef { pub catalogue: String, pub project: String, pub version: String }   // "ftb" | "modrinth" | "curseforge"
pub struct ManagedFile { pub path: String, pub sha1: String }   // path relative to game/, forward slashes; sha1 as written by us
```

  - `pub fn slug(name: &str) -> String`: lower case, every run of characters outside `a-z0-9` becomes one `-`, trimmed of `-`; empty result becomes `instance`.
  - `pub fn create(paths: &Paths, name: &str, minecraft: &str, loader: Option<Loader>, source: Source) -> Result<Instance>`: id is `slug(name)`, with `-2`, `-3`, ... while that folder exists; makes `instances/<id>/game/mods/` and writes `instance.json`.
  - `pub fn load(paths: &Paths, id: &str) -> Result<Instance>` (missing: `Error::Other("no instance <id>")`), `pub fn save(paths: &Paths, instance: &Instance) -> Result<()>` (write `instance.json.tmp`, then rename), `pub fn list(paths: &Paths) -> Result<Vec<Instance>>` (sorted by name; a folder without a readable `instance.json` is skipped), `pub fn delete(paths: &Paths, id: &str) -> Result<()>` (removes the whole folder; the id must equal `slug(id)`, so it can never name a path).
  - `pub fn game_dir(paths: &Paths, id: &str) -> PathBuf`.
  - `pub fn mods(paths: &Paths, instance: &Instance) -> Result<Vec<ModFile>>` with `ModFile { name: String, size: u64, managed: bool }`: the `.jar` and `.jar.disabled` files directly in `game/mods/`.
  - `pub fn add_mod(paths: &Paths, instance: &Instance, file: &Path) -> Result<()>`: copies a `.jar` into `game/mods/`; an existing file of that name is `Error::Other("<name> is already in this instance")`. Never touches `managed`.
  - `pub fn remove_mod(paths: &Paths, instance: &mut Instance, name: &str) -> Result<()>`: `name` must be a plain file name (no `/`, `\`, `..`); deletes it and drops it from `managed` (saving), so a sync may bring a pack's mod back.

- [ ] **Step 1: Write the failing tests** (each on its own temp root):

```rust
#[test] fn slugs() {
    assert_eq!(slug("All the Mods 10!"), "all-the-mods-10");
    assert_eq!(slug("  ../..  "), "instance");
}
#[test] fn create_load_list_delete() {
    let a = create(&p, "My Pack", "1.21.1", Some(Loader::parse("neoforge:21.1.90").unwrap()), Source::Manual).unwrap();
    let b = create(&p, "My Pack", "1.21.1", None, Source::Manual).unwrap();
    assert_eq!((a.id.as_str(), b.id.as_str()), ("my-pack", "my-pack-2"));
    assert!(game_dir(&p, "my-pack").join("mods").is_dir());
    assert_eq!(load(&p, "my-pack").unwrap().loader.unwrap().version.as_deref(), Some("21.1.90"));
    assert_eq!(list(&p).unwrap().len(), 2);
    delete(&p, "my-pack").unwrap();
    assert!(load(&p, "my-pack").is_err());
    assert!(delete(&p, "..").is_err());
}
#[test] fn source_round_trips() {
    // Source::Server { panel: "p1", server: "atm10", pack: Some(PackRef{..}) } -> save -> load -> equal fields
}
#[test] fn own_mods() {
    add_mod(&p, &inst, &jar_named("own.jar")).unwrap();
    assert!(add_mod(&p, &inst, &jar_named("own.jar")).is_err());
    let m = mods(&p, &inst).unwrap();
    assert!(m.iter().any(|f| f.name == "own.jar" && !f.managed));
    assert!(remove_mod(&p, &mut inst, r"..\instance.json").is_err());
    remove_mod(&p, &mut inst, "own.jar").unwrap();
    assert!(mods(&p, &inst).unwrap().is_empty());
}
```

- [ ] **Step 2:** Run `cargo test -p portal-core instance`, see them fail; implement; run: PASS.
- [ ] **Step 3: Commit** `Add instances with their own mods`.

---

### Task 2: Deciding a sync

**Files:**
- Create: `crates/core/src/sync.rs`

**Interfaces:**
- Consumes: `ManagedFile`, `Hash`, `sha1_file`, `file_matches`.
- Produces:

```rust
pub struct Wanted { pub path: String, pub from: Origin, pub hash: Option<Hash>, pub size: Option<u64> }
pub enum Origin {
    Url(String),
    Bytes(Vec<u8>),                       // a file out of a pack archive
    Manual { name: String, page: String } // the player has to fetch it; page may be empty
}
pub struct Manual { pub path: String, pub name: String, pub page: String }
pub struct SyncPlan {
    pub write: Vec<Wanted>,        // to download or write
    pub delete: Vec<String>,       // managed, unchanged, no longer wanted
    pub managed: Vec<ManagedFile>, // what stays managed without being touched
    pub manual: Vec<Manual>,       // wanted, not on disk, cannot be downloaded
}
pub fn safe_path(path: &str) -> Result<String>
pub fn plan(game_dir: &Path, managed: &[ManagedFile], wanted: Vec<Wanted>) -> Result<SyncPlan>
```

`safe_path`: turns `\` into `/`, strips a leading `./`, and refuses (`Error::Other("unsafe path in pack: <path>")`) an empty path, a leading `/`, a `:` anywhere, or a `..` segment.

`plan`, per wanted file (after `safe_path`; a path wanted twice: the later one wins):

| On disk | In `managed` | Result |
| --- | --- | --- |
| no | either | `write` (or `manual` for `Origin::Manual`) |
| yes, equals the wanted hash | either | nothing to do; goes to `managed` with its SHA-1 |
| yes, SHA-1 equals the managed one | yes | `write` (ours, unchanged, outdated) |
| yes, SHA-1 differs from the managed one | yes | kept; dropped from `managed` (it is the player's now) |
| yes | no | kept; not managed (the player's own file) |

A `Origin::Manual` file that is on disk counts as "nothing to do" and becomes managed with the SHA-1 it has. A wanted file without a hash that is on disk and managed and unchanged is left alone and stays managed.

Then, per managed file that is not wanted: unchanged on disk → `delete`; changed → kept, dropped from `managed`; missing → dropped.

- [ ] **Step 1: Write the failing tests.** A helper builds a game folder from `[(path, bytes)]`, `m(path, bytes)` makes the `ManagedFile` with the SHA-1 of `bytes`, `w(path, bytes)` a `Wanted` with `Origin::Url("u")` and `Hash::Sha1` of `bytes`.

```rust
#[test] fn safe_paths() {
    assert_eq!(safe_path(r".\mods\a.jar").unwrap(), "mods/a.jar");
    for bad in ["", "/etc/x", r"C:\Windows\x", "mods/../../x", "..", r"..\x"] { assert!(safe_path(bad).is_err()); }
}
#[test] fn a_new_file_is_written()               // disk {}            wanted a      -> write [a]
#[test] fn a_right_file_is_adopted()             // disk {a=1}         wanted a=1    -> write [], managed [a]
#[test] fn our_outdated_file_is_replaced()       // disk {a=1} m(a,1)  wanted a=2    -> write [a]
#[test] fn a_changed_managed_file_is_kept()      // disk {a=X} m(a,1)  wanted a=2    -> write [], delete [], managed []
#[test] fn the_players_own_file_is_kept()        // disk {a=X}         wanted a=2    -> write [], managed []
#[test] fn an_unwanted_managed_file_is_deleted() // disk {a=1} m(a,1)  wanted none   -> delete [a]
#[test] fn an_unwanted_changed_file_is_kept()    // disk {a=X} m(a,1)  wanted none   -> delete [], managed []
#[test] fn an_unmanaged_file_is_never_deleted()  // disk {own=1}       wanted none   -> delete []
#[test] fn a_manual_file_is_listed_until_it_is_there() // Origin::Manual: missing -> manual [a]; present -> manual [], managed [a]
#[test] fn an_unsafe_path_fails_the_whole_plan() // wanted [good, "../x"] -> Err
```

- [ ] **Step 2:** Run `cargo test -p portal-core sync`, see them fail; implement `safe_path` and `plan`; run: PASS.
- [ ] **Step 3: Commit** `Decide what a sync writes, deletes and keeps`.

---

### Task 3: Applying a sync

**Files:**
- Modify: `crates/core/src/sync.rs`

**Interfaces:**
- Consumes: `SyncPlan`, `fetch_all`, `Download`, `instance::save`.
- Produces: `pub async fn apply(client: &Client, paths: &Paths, instance: &mut Instance, plan: SyncPlan, progress: &ProgressTx) -> Result<Vec<Manual>>`

In this order:
1. Delete the `plan.delete` files; an already missing file is fine. Remove folders this leaves empty, up to but not including `game/`.
2. Write `Origin::Bytes` files; download `Origin::Url` files with `fetch_all` (stage `mods`).
3. Set `instance.managed` to `plan.managed` plus each written file with its real `sha1_file`; save.
4. Return `plan.manual`.

When step 2 fails, run step 3 for the files that did arrive (those whose file exists and, when a hash was wanted, matches it), save, and return the error. A process killed before step 3 leaves the old `instance.json`: the next `plan` drops the deleted files and adopts the new ones by their hash, so nothing is lost and nothing of the player's is at risk.

- [ ] **Step 1: Write the failing tests** (with the test file server of plan 1's download tests; move that helper to a `#[cfg(test)] pub mod testing` in `download.rs`):

```rust
#[tokio::test] async fn apply_writes_deletes_and_records() {
    // managed old.jar on disk; plan: write new.jar (Url) and config/a.toml (Bytes), delete old.jar
    // after: old.jar gone, both new files there, instance.json lists exactly the two with their sha1
}
#[tokio::test] async fn a_failed_download_keeps_what_arrived() {
    // write good.jar (served) and bad.jar (404) -> Err; instance.json lists good.jar, not bad.jar
}
```

- [ ] **Step 2:** Run, see them fail; implement `apply`; run: PASS.
- [ ] **Step 3: Commit** `Apply a sync and record what it installed`.

---

### Task 4: The FTB installer

**Files:**
- Create: `crates/core/src/pack.rs`, `crates/core/src/pack/ftb.rs`
- Create: `crates/core/tests/fixtures/ftb-pack-version.json`

**Interfaces:**
- Consumes: `Wanted`, `Origin`, `Loader`, `LoaderKind`.
- Produces:
  - in `pack.rs`: `pub struct PackInstall { pub minecraft: String, pub loader: Option<Loader>, pub files: Vec<Wanted> }`
  - in `ftb.rs`: `pub fn parse(json: &str) -> Result<PackInstall>` and `pub async fn resolve(client: &Client, pack: &str, version: &str) -> Result<PackInstall>` (`GET https://api.feed-the-beast.com/v1/modpacks/public/modpack/<pack>/<version>`; a body with `"status": "error"` is `Error::Other` with its `message`).

`parse`: `targets` holds `{ name, type, version }`; the one with `type` `game` (name `minecraft`) gives the Minecraft version, the one with `type` `modloader` the loader (`LoaderKind::parse(name)`, with its `version`). No game target: an error. Each of `files` with `serveronly: true` is skipped; the others become `Wanted { path: <path> + <name>, hash: Sha1 (when not empty), size }` with `Origin::Url(url)`, or, when `url` is empty, `Origin::Manual { name, page }` where `page` is `https://www.curseforge.com/projects/<curseforge.project>` when the file has a `curseforge` object and empty otherwise.

- [ ] **Step 1: Save the fixture.** Fetch a current pack version from the URL above (the owner's live server runs pack `126`; take the version id the panel shows) and trim `files` by hand to: two mods, one config file, one `serveronly` file, one `clientonly` file, and one copy of a mod edited to have `"url": ""`. Keep `targets` whole.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn ftb_pack() {
    let p = parse(include_str!("../../tests/fixtures/ftb-pack-version.json")).unwrap();
    assert_eq!(p.minecraft, /* fixture's game target */);
    assert!(matches!(p.loader.as_ref().unwrap().kind, LoaderKind::NeoForge));
    assert!(p.loader.unwrap().version.is_some());
    assert_eq!(p.files.len(), 5);                                    // 6 in the fixture, 1 server-only
    assert!(p.files.iter().any(|f| f.path.starts_with("./mods/") || f.path.starts_with("mods/")));
    assert!(p.files.iter().any(|f| f.path.contains("config/")));
    assert_eq!(p.files.iter().filter(|f| matches!(f.from, Origin::Manual { .. })).count(), 1);
    assert!(p.files.iter().all(|f| !f.path.contains(/* the server-only file's name */)));
}
#[test] fn an_api_error_is_reported() {
    assert!(parse(r#"{"status":"error","message":"Modpack not found"}"#).unwrap_err().to_string().contains("Modpack not found"));
}
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Read an FTB pack version`.

---

### Task 5: The Modrinth installer

**Files:**
- Create: `crates/core/src/pack/modrinth.rs`
- Create: `crates/core/tests/fixtures/modrinth.index.json`

**Interfaces:**
- Produces:
  - `pub fn parse_index(json: &str) -> Result<PackInstall>` (the files of the index only).
  - `pub fn read_mrpack(archive: &Path) -> Result<PackInstall>`: `parse_index` of `modrinth.index.json` in the zip, plus every file under `overrides/` and then `client-overrides/` as `Origin::Bytes` with the folder prefix removed (so a client override replaces a plain one). `server-overrides/` is ignored.
  - `pub async fn resolve(client: &Client, paths: &Paths, version: &str) -> Result<PackInstall>`: `GET https://api.modrinth.com/v2/version/<version>`; take the entry of `files` with `primary: true` (else the first); download it to `meta/packs/modrinth-<version>.mrpack` checked against `hashes.sha512` (add `Paths::packs()`); `read_mrpack`. A 404 is `Error::Other("Modrinth has no version <version>")`.

`parse_index`: `dependencies.minecraft` is the Minecraft version; the loader is the first present of `neoforge`, `forge`, `fabric-loader` (kind Fabric), with its value as version; `quilt-loader` is `Error::Unsupported`. A file whose `env.client` is `unsupported` is skipped; the others become `Wanted { path, from: Url(downloads[0]), hash: Sha512(hashes.sha512), size: fileSize }`.

Only the version id is needed: a version belongs to one project, so the project (which the panel may send as a pasted link) is never looked up.

- [ ] **Step 1: Save the fixture:** `modrinth.index.json` out of a real `.mrpack`, trimmed to three files, one of them edited to `"env": { "client": "unsupported", "server": "required" }`.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn index() {
    let p = parse_index(include_str!("../../tests/fixtures/modrinth.index.json")).unwrap();
    assert_eq!(p.files.len(), 2);
    assert!(p.files.iter().all(|f| matches!(f.hash, Some(Hash::Sha512(_))) && matches!(f.from, Origin::Url(_))));
    assert!(p.loader.is_some());
}
#[test] fn quilt_is_refused()   // dependencies {"minecraft":"1.21.1","quilt-loader":"0.26.0"} -> Err(Unsupported)
#[test] fn overrides() {
    // a zip built in the test: the index fixture, overrides/config/a.toml ("plain"), client-overrides/config/a.toml ("client"),
    // overrides/options.txt, server-overrides/server.properties
    let p = read_mrpack(&zip).unwrap();
    let a: Vec<_> = p.files.iter().filter(|f| f.path == "config/a.toml").collect();
    assert!(matches!(&a.last().unwrap().from, Origin::Bytes(b) if b == b"client"));
    assert!(p.files.iter().any(|f| f.path == "options.txt"));
    assert!(!p.files.iter().any(|f| f.path.contains("server.properties")));
}
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Read a Modrinth pack`.

---

### Task 6: Installing a pack, launching an instance, the commands

**Files:**
- Modify: `crates/core/src/pack.rs`, `crates/core/src/play.rs`, `crates/core/src/main.rs`, `README.md`

**Interfaces:**
- Consumes: everything above; `play::start`.
- Produces:
  - in `pack.rs`: `pub async fn resolve(client: &Client, paths: &Paths, pack: &PackRef) -> Result<PackInstall>` (`ftb` and `modrinth`; `curseforge` is `Error::Other("a CurseForge pack needs a panel's server")` until plan 4) and `pub async fn install(client: &Client, paths: &Paths, instance: &mut Instance, pack: PackInstall, progress: &ProgressTx) -> Result<Vec<Manual>>`: sets the instance's `minecraft` and `loader` from the pack, then `sync::plan` and `sync::apply`.
  - in `play.rs`: `pub async fn start_instance(client: &Client, paths: &Paths, instance: &Instance, profile: Profile, server: Option<String>, progress: &ProgressTx) -> Result<Child>`: `start` with the instance's version, loader and `game_dir`, memory `DEFAULT_MEMORY_MB`.

- [ ] **Step 1:** Implement both. No new unit test: they string tested parts together.
- [ ] **Step 2: Commands** in `main.rs`:
  - `portal instance list` (id, name, version, loader, source), `instance create <name> <version> [--loader l[:v]]`, `instance delete <id>`, `instance mods <id>` (name, size, `pack` or `own`), `instance add-mod <id> <jar>`, `instance remove-mod <id> <file name>`.
  - `portal pack install ftb <pack> <version> [--name <name>]` and `portal pack install modrinth <version id> [--name <name>]`: create the instance with `Source::Pack` (default name: `<catalogue> <project> <version>`), install, print each manual file as `fetch by hand: <name>  <page>  -> <mods folder>`.
  - `portal pack update <instance id> <version>`: resolve the instance's pack at the new version, install over it, store the new version in `source`.
  - `portal launch <instance id> [--name] [--server]` replaces plan 1's `launch <version> --dir`; `--loader` and `--dir` go away.
- [ ] **Step 3: Check by running.**

Run: `cargo run -p portal-core -- pack install ftb 126 <version id> --name "FTB test"`, then `portal launch ftb-test`
Expected: the instance fills, the game reaches the title screen with the pack's mods loaded.

Run: copy any extra mod jar in with `portal instance add-mod ftb-test <jar>`, edit one file under `game\config\`, then `portal pack update ftb-test <an older or newer version id>`
Expected: the extra jar and the edited config are untouched; `instance mods` shows the jar as `own`; mods the other version does not have are gone.

Run: `cargo run -p portal-core -- pack install modrinth <a version id of a Fabric pack>` and launch it.
Expected: title screen with the pack's mods; files from `overrides` are in `game\`.

- [ ] **Step 4:** Update the README's command list.
- [ ] **Step 5: Commit** `Install FTB and Modrinth packs into instances`.

---

## Carried over from plans 1 and 2

Found while building and reviewing the earlier plans; none blocks this plan, each is picked up where it is first felt.

- Interfaces as built: `loader::resolve(client, minecraft, &Loader) -> Result<Loader>` turns a loader without a version into one with it, before anything is downloaded; `play::start` calls it. An instance created with a loader stores the resolved version, so it does not move to a newer loader on a later launch. `StartRequest` has `loader: Option<Loader>`.
- Every name from a downloaded JSON or archive that becomes a path goes through `paths::check_relative` (or `enclosed_name()` for zip entries); this plan's `sync::safe_path` is the same rule for game-folder paths, so build it on `check_relative` rather than beside it. `check_relative` lets `?`, `#`, `%` and `:` inside a name through: a pack id or version put into a URL must be checked for those too.
- `download::get_ok` is the one status-checked GET; `download::fetch` the one hash-checked download (a file without a hash is only checked for existing).
- `Error::Hash`'s message names no algorithm (Modrinth files are SHA-512).
- A modded launch installs the vanilla files and then the merged version, so vanilla assets are hashed twice; split the library part out of `gamefiles::install` when it is felt (plan 5).
- Processors are started with `java.exe`; from a window that needs `CREATE_NO_WINDOW` (plan 5). Hashing, zip reading and natives extraction block the async runtime; `spawn_blocking` if progress stalls (plan 5).
- `java::ensure` needs the network even when the runtime is installed; decide before plan 5. `Error` is not `Serialize` (plan 5).
- `launch::fill` replaces placeholders one after another; make it a single pass before real tokens arrive (plan 6).
- `play.rs` has a `_ => version` arm that cannot be reached after `resolve`; remove it or make it an error when that code is next touched.
- NeoForge version resolving only filters `-beta` builds; `library_files` skips names ending in `-arm64` or `-x86`.
- `cargo fmt` has not been run on the crate.
