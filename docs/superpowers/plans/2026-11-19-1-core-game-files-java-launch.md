# Core: Game Files, Java, Offline Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `portal launch 1.21.1 --dir <folder>` downloads vanilla Minecraft and Mojang's Java and starts the game with an offline profile.

**Architecture:** One library crate `portal-core` in `crates/core` with a small `main.rs` (binary `portal`) beside it. Parsing and deciding (rules, arguments) are pure functions tested with saved JSON; downloading and launching are checked by running the binary. No Tauri anywhere in this crate.

**Tech Stack:** Rust (stable, MSVC), `reqwest`, `tokio`, `serde`, `serde_json`, `sha1`, `sha2`, `zip`.

**Spec:** `docs/superpowers/specs/2026-11-19-spawnpoint-portal-design.md` (pieces 1 to 3 of "Core, piece by piece"; item 1 of "Order of work").

## Global Constraints

- Windows 10/11 x64 only. OS name `windows`, arch `x86_64`/`x64` wherever a rule or manifest asks.
- Crates allowed in `portal-core`: `reqwest`, `tokio`, `serde`, `serde_json`, `sha1`, `sha2`, `zip`, `keyring`. Nothing else, also not as a dev-dependency (no `clap`, `anyhow`, `thiserror`, `hex`, `tempfile`, `futures`). Ask the owner before adding one.
- Data root: `%APPDATA%\SpawnpointPortal\`. The environment variable `SPAWNPOINT_PORTAL_HOME` overrides it (tests and experiments).
- Every downloaded file that has a SHA-1 is checked against it; a file that is already right is not fetched again.
- Minecraft older than 1.13 is refused with a clear message (a version JSON without `arguments`).
- Every HTTP request carries the User-Agent `spawnpoint-portal/<crate version>`. Never put a name, e-mail or machine name in it.
- Every public data type derives `Debug, Clone, Serialize, Deserialize` (the window sends them over Tauri later).
- API shapes below come from documentation read in November 2026. Each task that parses a response first saves a real one as a fixture; if it differs from what the task describes, the fixture wins and the task is adjusted.
- Comments: at most three lines, only for a non-obvious why. No banner or block comments.
- Commit messages: never an `@`-prefixed token, never a mention of an AI or assistant, no co-author trailer.

## Review Focus

- A download that breaks halfway (connection drop, process killed): the half file must not be taken for a good one on the next run. Pinned in Task 2 (`.part` file, rename after the hash check).
- A file on disk with the right name and the wrong content: fetched again. Pinned in Task 2.
- A game folder or data root with a space in its path (`C:\Users\Jan Peeters\...`): the game still starts. Pinned in Task 6 (arguments are passed as separate values, never joined into one string; a test uses a path with a space).
- A version id that does not exist (`portal launch 1.99`): one line naming the id, no panic. Pinned in Task 4.
- A library that carries rules for another OS, or natives for macOS only: skipped, not downloaded, not on the classpath. Pinned in Task 3.

## File Structure

```
Cargo.toml                      workspace: members = ["crates/core"]
.gitignore                      target/, node_modules/, dist/
crates/core/Cargo.toml          package portal-core, [[bin]] name = "portal"
crates/core/src/lib.rs          module list, http_client()
crates/core/src/error.rs        Error, Result
crates/core/src/paths.rs        Paths: where everything lives on disk
crates/core/src/download.rs     hashing, one download, many downloads with progress
crates/core/src/version.rs      serde types of a version JSON, rules, maven coordinates
crates/core/src/gamefiles.rs    manifest, version JSON, client jar, libraries, assets, log config
crates/core/src/java.rs         Mojang's Java runtime
crates/core/src/launch.rs       classpath, arguments, natives, Profile
crates/core/src/play.rs         the whole chain: files, Java, start
crates/core/src/main.rs         command line
crates/core/tests/fixtures/     saved JSON
```

Tests live in a `#[cfg(test)] mod tests` at the bottom of the file they test and read fixtures with `include_str!("../tests/fixtures/<name>")`. A test that needs a folder makes one under `std::env::temp_dir()` with a unique name and removes it at the end.

---

### Task 1: Toolchain, workspace, errors, paths

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `crates/core/Cargo.toml`, `crates/core/src/{lib.rs,error.rs,paths.rs,main.rs}`

**Interfaces:**
- Produces:
  - `pub enum Error { Io(std::io::Error), Http(reqwest::Error), Status { url: String, status: u16 }, Json(serde_json::Error), Zip(zip::result::ZipError), Hash { path: PathBuf, expected: String, actual: String }, Unsupported(String), Other(String) }` with `Display`, `std::error::Error` and `From` for the wrapped types; `pub type Result<T> = std::result::Result<T, Error>`.
  - `pub struct Paths { pub root: PathBuf }` with `Paths::new() -> Result<Paths>` (`SPAWNPOINT_PORTAL_HOME`, else `%APPDATA%\SpawnpointPortal`), and methods returning `PathBuf`: `versions()` = `meta/versions`, `libraries()` = `meta/libraries`, `assets()` = `meta/assets`, `natives(version_id: &str)` = `meta/natives/<id>`, `java()` = `java`, `instances()` = `instances`, `panels_file()` = `panels.json`.
  - `pub fn http_client() -> reqwest::Client` in `lib.rs`: User-Agent `spawnpoint-portal/` + `env!("CARGO_PKG_VERSION")`, connect timeout 15 s, no overall timeout (large files).

- [ ] **Step 1: Install the toolchain.** Rust is not on this machine. Install the Visual Studio 2022 Build Tools with the "Desktop development with C++" workload, then `rustup` with the default `stable-x86_64-pc-windows-msvc` toolchain. Both installers need the owner at the keyboard: ask them to run `winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"` and `winget install Rustlang.Rustup`, then open a new terminal.

Run: `cargo --version; rustc --version`
Expected: two version lines, toolchain `msvc`.

- [ ] **Step 2: Create the workspace.** Root `Cargo.toml` with `[workspace] resolver = "2"`, `members = ["crates/core"]`. `crates/core/Cargo.toml`: package `portal-core`, edition 2024, `[lib]` default, `[[bin]] name = "portal" path = "src/main.rs"`. Dependencies: `reqwest` (features `json`, `rustls-tls`, `default-features = false`), `tokio` (features `rt-multi-thread`, `macros`, `fs`, `sync`, `process`, `time`), `serde` (feature `derive`), `serde_json`, `sha1`, `sha2`, `zip` (`default-features = false`, feature `deflate`). `keyring` is added by the plan that first needs it.

- [ ] **Step 3: Write the failing test** in `paths.rs`:

```rust
#[test]
fn paths_follow_the_override() {
    // SAFETY: the only test that touches this variable.
    unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", r"C:\tmp\portal home") };
    let p = Paths::new().unwrap();
    assert_eq!(p.versions(), PathBuf::from(r"C:\tmp\portal home\meta\versions"));
    assert_eq!(p.natives("1.16.5"), PathBuf::from(r"C:\tmp\portal home\meta\natives\1.16.5"));
    assert_eq!(p.panels_file(), PathBuf::from(r"C:\tmp\portal home\panels.json"));
}
```

- [ ] **Step 4:** Run `cargo test -p portal-core paths` and see it fail to compile.
- [ ] **Step 5:** Implement `error.rs`, `paths.rs`, `http_client()`, and a `main.rs` that prints `portal <version>`.
- [ ] **Step 6:** Run `cargo test -p portal-core` (PASS) and `cargo run -p portal-core` (prints the version line).
- [ ] **Step 7: Commit** `Add the core crate with its errors and paths`.

---

### Task 2: Downloads

**Files:**
- Create: `crates/core/src/download.rs`

**Interfaces:**
- Consumes: `Error`, `Result`.
- Produces:
  - `pub enum Hash { Sha1(String), Sha512(String) }` (lower-case hex).
  - `pub fn sha1_file(path: &Path) -> Result<String>`, `pub fn sha512_file(path: &Path) -> Result<String>`, `pub fn file_matches(path: &Path, hash: &Hash) -> bool` (false when the file is missing).
  - `pub struct Download { pub url: String, pub path: PathBuf, pub hash: Option<Hash>, pub size: Option<u64> }`.
  - `pub struct Progress { pub stage: String, pub done: u64, pub total: u64 }` (files, not bytes) and `pub type ProgressTx = tokio::sync::mpsc::UnboundedSender<Progress>`.
  - `pub async fn fetch(client: &Client, item: &Download) -> Result<()>`.
  - `pub async fn fetch_all(client: &Client, stage: &str, items: Vec<Download>, progress: &ProgressTx) -> Result<()>`.
  - `pub const CONCURRENT_DOWNLOADS: usize = 16; pub const ATTEMPTS: u32 = 3;`

Behaviour that the signatures do not say:
- `fetch` returns at once when `hash` is set and the file matches; without a hash, when the file exists and `size` matches (or `size` is `None`).
- It streams into `<path>.part` with `Response::chunk()`, checks the hash, then renames over `path`. A mismatch deletes the `.part` and is `Error::Hash`. A non-2xx status is `Error::Status`.
- It tries `ATTEMPTS` times with a pause of 1 s then 2 s; `Error::Status` with a 4xx is not retried.
- `fetch_all` runs at most `CONCURRENT_DOWNLOADS` at a time (`tokio::task::JoinSet` and a `Semaphore`), sends one `Progress` per finished file, stops at the first error. A send to a closed channel is ignored.

- [ ] **Step 1: Write the failing tests.** They serve files from a `tokio::net::TcpListener` on `127.0.0.1:0` written in the test module (a few lines: read the request, answer a fixed body with `Content-Length`), so no network and no extra crate.

```rust
#[test]
fn hashes_a_file() {
    // "abc"
    assert_eq!(sha1_file(&tmp_with(b"abc")).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
}
#[tokio::test] async fn a_right_file_is_not_fetched_again()   // server counts requests: 0 after fetch
#[tokio::test] async fn a_wrong_file_is_replaced()            // same name, other bytes -> right bytes after
#[tokio::test] async fn a_wrong_hash_leaves_no_file()         // Err(Error::Hash{..}); neither path nor path.part exists
#[tokio::test] async fn a_leftover_part_file_is_overwritten() // stale "x.part" present -> fetch succeeds
#[tokio::test] async fn fetch_all_reports_every_file()        // 5 items -> last Progress { done: 5, total: 5 }
#[tokio::test] async fn a_404_is_not_retried()                // server counts requests: 1
```

- [ ] **Step 2:** Run `cargo test -p portal-core download` and see them fail.
- [ ] **Step 3:** Implement `download.rs`. Hex is written with `format!("{:02x}")`; parent folders are created before writing.
- [ ] **Step 4:** Run `cargo test -p portal-core download`: PASS.
- [ ] **Step 5: Commit** `Add checked, resumable-by-rerun downloads`.

---

### Task 3: Version JSON and rules

**Files:**
- Create: `crates/core/src/version.rs`
- Create: `crates/core/tests/fixtures/version-1.21.1.json`, `version-1.16.5.json`, `version-1.12.2.json`

**Interfaces:**
- Produces:
  - `pub struct VersionJson { pub id: String, pub inherits_from: Option<String>, pub main_class: String, pub arguments: Option<Arguments>, pub asset_index: Option<AssetIndexRef>, pub assets: Option<String>, pub downloads: Option<Downloads>, pub libraries: Vec<Library>, pub java_version: Option<JavaVersion>, pub logging: Option<Logging>, pub minecraft_arguments: Option<String> }` (`#[serde(rename_all = "camelCase")]`; everything a loader's JSON may leave out is an `Option` or `#[serde(default)]`).
  - `pub struct Arguments { pub game: Vec<Argument>, pub jvm: Vec<Argument> }` (both `#[serde(default)]`), `#[serde(untagged)] pub enum Argument { Plain(String), Ruled { rules: Vec<Rule>, value: OneOrMany } }`, `#[serde(untagged)] pub enum OneOrMany { One(String), Many(Vec<String>) }`.
  - `pub struct Rule { pub action: String, pub os: Option<OsRule>, pub features: Option<BTreeMap<String, bool>> }`, `pub struct OsRule { pub name: Option<String>, pub arch: Option<String>, pub version: Option<String> }`.
  - `pub struct Library { pub name: String, pub downloads: Option<LibraryDownloads>, pub url: Option<String>, pub sha1: Option<String>, pub rules: Option<Vec<Rule>>, pub natives: Option<BTreeMap<String, String>>, pub extract: Option<Extract> }`, `LibraryDownloads { artifact: Option<Artifact>, classifiers: Option<BTreeMap<String, Artifact>> }`, `Artifact { path: Option<String>, sha1: Option<String>, size: Option<u64>, url: String }`, `Extract { exclude: Vec<String> }`.
  - `AssetIndexRef { id, sha1, url }`, `Downloads { client: Artifact }`, `JavaVersion { component: String, major_version: u32 }`, `Logging { client: Option<LoggingClient> }`, `LoggingClient { argument: String, file: LoggingFile { id, sha1, size, url } }`.
  - `pub type Features = BTreeSet<String>` (the features that are on).
  - `pub fn allowed(rules: &[Rule], features: &Features) -> bool`
  - `pub fn maven_path(coordinate: &str) -> Result<String>`: `group:artifact:version[:classifier][@ext]` to `group/with/slashes/artifact/version/artifact-version[-classifier].ext`, `ext` default `jar`.
  - `pub struct LibraryFile { pub path: String, pub url: String, pub sha1: Option<String>, pub size: Option<u64>, pub native: bool }`
  - `pub fn library_files(version: &VersionJson) -> Result<Vec<LibraryFile>>`: for every library whose rules allow Windows: its `downloads.artifact` (path from `artifact.path`, else `maven_path(name)`); when there is no `downloads`, `maven_path(name)` under `url` (default `https://libraries.minecraft.net/`) with the library's own `sha1`; and when `natives` has a `windows` key, the classifier it names (`${arch}` replaced by `64`) with `native: true`.

Rule semantics (not determined by the signature): start from "not allowed"; walk the rules in order; a rule applies when its `os.name` is absent or `windows`, its `os.arch` is absent or `x86_64`, and every entry of its `features` equals whether that feature is in `features`; the last rule that applies sets allowed to `action == "allow"`. An empty list is allowed. `os.arch` of `x86` does not apply; `os.version` is ignored (only used for old Windows and macOS quirks).

- [ ] **Step 1: Save the fixtures.** Take the `url` of each id from `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` and save the three version JSONs unchanged with `curl -o`.

- [ ] **Step 2: Write the failing tests.**

```rust
fn v(name: &str) -> VersionJson  // parses a fixture

#[test] fn parses_the_three_versions() {
    assert_eq!(v("1.21.1").main_class, "net.minecraft.client.main.Main");
    assert_eq!(v("1.21.1").java_version.unwrap().component, "java-runtime-delta");
    assert!(v("1.16.5").arguments.is_some());
    assert!(v("1.12.2").arguments.is_none() && v("1.12.2").minecraft_arguments.is_some());
}
#[test] fn rules() {
    let none = Features::new();
    assert!(allowed(&[], &none));
    assert!(allowed(&rules(r#"[{"action":"allow","os":{"name":"windows"}}]"#), &none));
    assert!(!allowed(&rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#), &none));
    assert!(!allowed(&rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"windows"}}]"#), &none));
    assert!(!allowed(&rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#), &none));
    let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
    assert!(!allowed(&demo, &none));
    assert!(allowed(&demo, &Features::from(["is_demo_user".to_string()])));
}
#[test] fn maven_paths() {
    assert_eq!(maven_path("org.ow2.asm:asm:9.6").unwrap(), "org/ow2/asm/asm/9.6/asm-9.6.jar");
    assert_eq!(maven_path("net.neoforged:neoforge:21.1.90:client").unwrap(), "net/neoforged/neoforge/21.1.90/neoforge-21.1.90-client.jar");
    assert_eq!(maven_path("de.oceanlabs.mcp:mcp_config:1.20.1@zip").unwrap(), "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip");
    assert!(maven_path("nonsense").is_err());
}
#[test] fn libraries_for_windows_only() {
    let files = library_files(&v("1.21.1")).unwrap();
    assert!(files.iter().any(|f| f.path.contains("lwjgl") && f.path.ends_with("natives-windows.jar")));
    assert!(!files.iter().any(|f| f.path.contains("macos") || f.path.contains("linux") || f.path.contains("arm64") || f.path.contains("x86.jar")));
    let old = library_files(&v("1.16.5")).unwrap();
    assert!(old.iter().any(|f| f.native && f.path.ends_with("natives-windows.jar")));
    assert!(!old.iter().any(|f| f.path.contains("natives-macos") || f.path.contains("natives-osx")));
}
```

- [ ] **Step 3:** Run `cargo test -p portal-core version` and see them fail.
- [ ] **Step 4:** Implement `version.rs`.
- [ ] **Step 5:** Run `cargo test -p portal-core version`: PASS.
- [ ] **Step 6: Commit** `Parse version JSON, rules and maven coordinates`.

---

### Task 4: Game files

**Files:**
- Create: `crates/core/src/gamefiles.rs`
- Modify: `crates/core/src/main.rs`
- Create: `crates/core/tests/fixtures/version_manifest_v2.json` (trimmed by hand to five entries, among them 1.21.1 and one snapshot), `asset-index.json` (trimmed to three objects)

**Interfaces:**
- Consumes: `Paths`, `fetch`, `fetch_all`, `Download`, `Hash`, `ProgressTx`, `VersionJson`, `library_files`.
- Produces:
  - `pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";`
  - `pub struct Manifest { pub latest: Latest, pub versions: Vec<ManifestVersion> }`, `Latest { release, snapshot }`, `ManifestVersion { id, kind (json "type"), url, sha1, release_time (json "releaseTime") }`.
  - `pub async fn manifest(client: &Client) -> Result<Manifest>`
  - `pub async fn version_json(client: &Client, paths: &Paths, id: &str) -> Result<VersionJson>`: reads `meta/versions/<id>/<id>.json` when it is there, else finds `id` in the manifest (unknown id: `Error::Other("Minecraft version <id> does not exist")`), downloads it checked against the manifest's `sha1`, and parses it.
  - `pub fn asset_downloads(index_json: &str, assets_dir: &Path) -> Result<Vec<Download>>`: every object `{hash, size}` becomes `https://resources.download.minecraft.net/<first two hex>/<hash>` at `objects/<first two hex>/<hash>`, once per hash.
  - `pub async fn install(client: &Client, paths: &Paths, version: &VersionJson, progress: &ProgressTx) -> Result<()>`: the client jar to `meta/versions/<id>/<id>.jar`, every `library_files` entry under `meta/libraries/<path>`, the asset index to `meta/assets/indexes/<assetIndex.id>.json`, its objects, and `logging.client.file` to `meta/assets/log_configs/<file.id>`. Stages named `libraries` and `assets`.

Plan 2 calls `install` with a loader's version merged over the vanilla one: its `id` is the loader's and its `inherits_from` is the vanilla id. The client jar therefore goes to `meta/versions/<j>/<j>.jar` with `j = inherits_from` when set, else `id` (the same rule as `launch::jar_path` in Task 6).

- [ ] **Step 1: Save the fixtures** from the manifest URL and from the `assetIndex.url` of 1.21.1, trimmed as described.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn parses_the_manifest() {
    let m: Manifest = serde_json::from_str(include_str!("../tests/fixtures/version_manifest_v2.json")).unwrap();
    assert!(m.versions.iter().any(|v| v.id == "1.21.1" && v.kind == "release"));
}
#[test] fn assets_become_downloads() {
    let d = asset_downloads(include_str!("../tests/fixtures/asset-index.json"), Path::new("A")).unwrap();
    assert_eq!(d.len(), 3);
    let h = /* first hash in the fixture */;
    assert_eq!(d[0].url, format!("https://resources.download.minecraft.net/{}/{}", &h[..2], h));
    assert_eq!(d[0].path, Path::new("A").join("objects").join(&h[..2]).join(h));
}
#[test] fn the_same_hash_is_downloaded_once()  // an index where two names share a hash -> 1 download
```

- [ ] **Step 3:** Run `cargo test -p portal-core gamefiles` and see them fail.
- [ ] **Step 4:** Implement `gamefiles.rs`.
- [ ] **Step 5:** Run the tests: PASS.
- [ ] **Step 6: Add two commands to `main.rs`.** Arguments are read by hand from `std::env::args()`. `portal versions` prints the release ids, newest first. `portal files <version>` runs `version_json` and `install`, printing each `Progress` as `<stage> <done>/<total>` on one rewritten line. An error prints `error: <Display>` and exits with code 1. Unknown command: a usage text listing the commands.
- [ ] **Step 7: Check by running.**

Run: `cargo run -p portal-core -- files 1.21.1`, then the same again.
Expected: first run downloads (several thousand assets); second run finishes in a few seconds without fetching. `%APPDATA%\SpawnpointPortal\meta\versions\1.21.1\1.21.1.jar` exists.

Run: `cargo run -p portal-core -- files 1.99`
Expected: `error: Minecraft version 1.99 does not exist`, exit code 1.

- [ ] **Step 8: Commit** `Download the game files of a version`.

---

### Task 5: Java

**Files:**
- Create: `crates/core/src/java.rs`
- Modify: `crates/core/src/main.rs`
- Create: `crates/core/tests/fixtures/java-all.json` (the `windows-x64` object only), `java-runtime-files.json` (trimmed to one directory, two files, one of them `bin/javaw.exe`)

**Interfaces:**
- Consumes: `Paths`, `fetch_all`, `Download`, `Hash`, `ProgressTx`.
- Produces:
  - `pub const RUNTIMES_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";`
  - `pub fn runtime_manifest_url(all_json: &str, component: &str) -> Result<String>`: `["windows-x64"][component][0].manifest.url`; a missing component is `Error::Other("no Java runtime <component> for Windows")`.
  - `pub fn runtime_downloads(files_json: &str, dir: &Path) -> Result<Vec<Download>>`: every entry of `files` with `"type": "file"` becomes its `downloads.raw` (`url`, `sha1`, `size`) at `dir/<key>`. Directories and links are skipped (directories are made by the download). The `lzma` variant is never used: decoding it would need a crate.
  - `pub async fn ensure(client: &Client, paths: &Paths, component: &str, progress: &ProgressTx) -> Result<PathBuf>`: fills `java/<component>/` (stage `java`) and returns `java/<component>/bin/javaw.exe`.

- [ ] **Step 1: Save the fixtures** from `RUNTIMES_URL` and from the manifest URL it gives for `java-runtime-delta`.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn finds_the_runtime() {
    let url = runtime_manifest_url(include_str!("../tests/fixtures/java-all.json"), "java-runtime-delta").unwrap();
    assert!(url.starts_with("https://piston-meta.mojang.com/v1/packages/"));
    assert!(runtime_manifest_url(include_str!("../tests/fixtures/java-all.json"), "nope").is_err());
}
#[test] fn files_only() {
    let d = runtime_downloads(include_str!("../tests/fixtures/java-runtime-files.json"), Path::new("J")).unwrap();
    assert_eq!(d.len(), 2);
    assert!(d.iter().any(|x| x.path == Path::new("J").join("bin").join("javaw.exe")));
    assert!(d.iter().all(|x| matches!(x.hash, Some(Hash::Sha1(_)))));
}
```

- [ ] **Step 3:** Run, see them fail; implement `java.rs`; run: PASS.
- [ ] **Step 4:** Add `portal java <component>` to `main.rs`, printing the path `ensure` returns.
- [ ] **Step 5: Check by running.**

Run: `cargo run -p portal-core -- java java-runtime-delta`, then `& "$env:APPDATA\SpawnpointPortal\java\java-runtime-delta\bin\java.exe" -version`
Expected: a Java 21 version line.

- [ ] **Step 6: Commit** `Fetch Mojang's Java runtime`.

---

### Task 6: Launch arguments and natives

**Files:**
- Create: `crates/core/src/launch.rs`

**Interfaces:**
- Consumes: `VersionJson`, `Argument`, `allowed`, `Features`, `library_files`, `Paths`.
- Produces:
  - `pub struct Profile { pub name: String, pub uuid: String, pub access_token: String, pub user_type: String }` and `Profile::offline(name: &str) -> Profile`: `user_type` `"legacy"`, `access_token` `"0"`, `uuid` the first 16 bytes of SHA-1 of `"OfflinePlayer:<name>"` as 32 hex characters. (Vanilla derives it with MD5; a server in offline mode computes its own, so any stable value does here, and MD5 would need a crate.)
  - `pub struct LaunchOptions { pub game_dir: PathBuf, pub profile: Profile, pub server: Option<String>, pub memory_mb: u32 }` (`server` is a hostname; the port is always 25565).
  - `pub const DEFAULT_MEMORY_MB: u32 = 4096;`
  - `pub fn jar_path(paths: &Paths, version: &VersionJson) -> PathBuf` = `meta/versions/<j>/<j>.jar` with `j = inherits_from` when set, else `id`. `gamefiles::install` uses this function.
  - `pub fn classpath(paths: &Paths, version: &VersionJson) -> Result<Vec<PathBuf>>`: every non-native `library_files` path under `meta/libraries`, then `jar_path`.
  - `pub fn arguments(paths: &Paths, version: &VersionJson, opts: &LaunchOptions) -> Result<Vec<String>>`: everything after the Java executable.
  - `pub fn extract_natives(paths: &Paths, version: &VersionJson) -> Result<()>`: unzips every `native: true` library into `paths.natives(&version.id)`, skipping folders, `META-INF/` and each library's `extract.exclude` prefixes; a file already there with the same size is left.
  - `pub fn command(java: &Path, args: &[String], game_dir: &Path) -> std::process::Command`: working directory `game_dir`, each argument passed with `.arg()`.

What `arguments` returns, in order:
1. `-Xmx<memory_mb>M`.
2. `logging.client.argument` with `${path}` replaced by `meta/assets/log_configs/<file.id>`, when the version has `logging`.
3. `arguments.jvm`, each allowed entry, placeholders filled.
4. `mainClass`.
5. `arguments.game`, each allowed entry, placeholders filled.
6. The server: when `opts.server` is set and `arguments.game` contains a rule on the feature `is_quick_play_multiplayer`, that feature is on (the JSON's own entry then yields `--quickPlayMultiplayer`); otherwise append `--server <host> --port 25565`. No other feature is ever on.

A version without `arguments` is `Error::Unsupported("Minecraft older than 1.13 is not supported yet")`.

Placeholders: `${auth_player_name}`, `${version_name}` (id), `${game_directory}`, `${assets_root}`, `${assets_index_name}` (`assets`), `${auth_uuid}`, `${auth_access_token}`, `${clientid}` (empty), `${auth_xuid}` (empty), `${user_type}`, `${version_type}` (`release`), `${natives_directory}`, `${launcher_name}` (`spawnpoint-portal`), `${launcher_version}` (crate version), `${classpath}` (joined with `;`), `${classpath_separator}` (`;`), `${library_directory}`, `${quickPlayMultiplayer}` (`<host>:25565`). A placeholder that is not in this list is left as it is.

- [ ] **Step 1: Write the failing tests** (fixtures from Task 3; `Paths { root: r"C:\data root".into() }`, game dir `C:\games\my instance`, profile `Profile::offline("Steve")`):

```rust
#[test] fn vanilla_1_21_1() {
    let a = arguments(&paths, &v("1.21.1"), &opts(None)).unwrap();
    assert_eq!(a[0], "-Xmx4096M");
    let main = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
    let cp = a.iter().position(|x| x == "-cp").unwrap();
    assert!(cp < main);
    assert!(a[cp + 1].ends_with(r"meta\versions\1.21.1\1.21.1.jar") && a[cp + 1].contains(';'));
    assert_eq!(after(&a, "--username"), "Steve");
    assert_eq!(after(&a, "--gameDir"), r"C:\games\my instance");   // one value, space intact
    assert_eq!(after(&a, "--assetIndex"), v("1.21.1").assets.unwrap());
    assert!(!a.iter().any(|x| x.contains("${")));
    assert!(!a.iter().any(|x| x == "--demo" || x == "--width" || x.starts_with("--quickPlay")));
    assert!(!a.iter().any(|x| x == "-XstartOnFirstThread"));       // macOS only
    assert!(a.iter().any(|x| x.starts_with("-Dlog4j.configurationFile=")));
}
#[test] fn quick_play_from_1_20() {
    let a = arguments(&paths, &v("1.21.1"), &opts(Some("mc.example.org"))).unwrap();
    assert_eq!(after(&a, "--quickPlayMultiplayer"), "mc.example.org:25565");
    assert!(!a.iter().any(|x| x == "--server"));
}
#[test] fn server_and_port_before_1_20() {
    let a = arguments(&paths, &v("1.16.5"), &opts(Some("mc.example.org"))).unwrap();
    assert_eq!(after(&a, "--server"), "mc.example.org");
    assert_eq!(after(&a, "--port"), "25565");
}
#[test] fn older_than_1_13_is_refused() {
    assert!(matches!(arguments(&paths, &v("1.12.2"), &opts(None)), Err(Error::Unsupported(_))));
}
#[test] fn offline_profile_is_stable() {
    assert_eq!(Profile::offline("Steve").uuid, Profile::offline("Steve").uuid);
    assert_eq!(Profile::offline("Steve").uuid.len(), 32);
    assert_ne!(Profile::offline("Steve").uuid, Profile::offline("Alex").uuid);
}
#[test] fn natives_skip_meta_inf()  // a zip built in the test with a.dll and META-INF/x -> only a.dll extracted
```

- [ ] **Step 2:** Run `cargo test -p portal-core launch` and see them fail.
- [ ] **Step 3:** Implement `launch.rs`.
- [ ] **Step 4:** Run the tests: PASS.
- [ ] **Step 5: Commit** `Build the launch arguments and extract natives`.

---

### Task 7: The whole chain and `portal launch`

**Files:**
- Create: `crates/core/src/play.rs`
- Modify: `crates/core/src/main.rs`, `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `pub struct StartRequest { pub minecraft: String, pub game_dir: PathBuf, pub profile: Profile, pub server: Option<String>, pub memory_mb: u32 }` (plan 2 adds `loader`).
  - `pub async fn start(client: &Client, paths: &Paths, request: &StartRequest, progress: &ProgressTx) -> Result<std::process::Child>`: `version_json`, `install`, `java::ensure` with the version's `javaVersion.component` (absent: `jre-legacy`), `extract_natives`, create `game_dir`, `arguments`, `command(...).spawn()`.

- [ ] **Step 1:** Implement `play.rs`. No unit test: it only strings tested parts together and is checked by running.
- [ ] **Step 2: Add `portal launch <version> --dir <folder> [--name <player>] [--server <host>] [--memory <MB>]`.** `--name` defaults to `Player`. It prints progress, then `started, pid <n>`, waits for the game to exit and exits with its code. `--dir` is required in this plan (plan 3 replaces it with instances).
- [ ] **Step 3: Check by running.**

Run: `cargo run -p portal-core -- launch 1.21.1 --dir "$env:TEMP\portal test\game"`
Expected: the Minecraft window opens and reaches the title screen, the player name is `Player`, and `options.txt` appears in the folder (whose path has a space).

Run: `cargo run -p portal-core -- launch 1.16.5 --dir "$env:TEMP\portal test\old"`
Expected: the title screen of 1.16.5 (Java 8 runtime `jre-legacy`, natives extracted to `meta\natives\1.16.5`).

Run: `cargo run -p portal-core -- launch 1.12.2 --dir "$env:TEMP\portal test\older"`
Expected: `error: Minecraft older than 1.13 is not supported yet`.

If a local server in offline mode is at hand: add `--server localhost` to the first command and expect the game to connect on its own.

- [ ] **Step 4:** Write the README: what the project is (two lines), how to build (`cargo build`), and the commands of `portal` so far.
- [ ] **Step 5: Commit** `Launch vanilla Minecraft offline from the command line`.
