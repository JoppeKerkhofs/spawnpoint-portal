// Horizon's own registry of reviewed mods: one index file, each mod file pinned by its SHA-256.
use std::cmp::Reverse;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;

use super::{Failure, Found, Hit, Installed, Outcome, plain_id};
use crate::download::{Download, Hash, Progress, ProgressTx, blocking, fetch, get_ok, sha256_file};
use crate::instance::{self, Instance};
use crate::sync::safe_path;
use crate::{Error, Paths, Result};

pub const INDEX: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
const FILES: &str = "https://github.com/horizon-rust/horizon-mods/releases/download/";

#[derive(Deserialize)]
struct Index {
    projects: Vec<Project>,
    #[serde(default)]
    revoked: Vec<Revoked>,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    kind: String,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    authors: Vec<String>,
    icon: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    versions: Vec<Version>,
}

#[derive(Deserialize)]
struct Version {
    game: String,
    file: File,
}

#[derive(Deserialize)]
struct File {
    name: String,
    url: String,
    size: Option<u64>,
    sha256: String,
}

// Without a hash the whole project is taken down, with one only that file.
#[derive(Deserialize)]
struct Revoked {
    id: String,
    sha256: Option<String>,
}

static CACHE: Mutex<Option<(String, Instant, Arc<Index>)>> = Mutex::new(None);

// The index is over a megabyte and a search asks for it on every keystroke: kept for ten minutes.
async fn index(client: &Client, url: &str) -> Result<Arc<Index>> {
    let cached = CACHE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some((_, _, index)) = cached.filter(|(from, at, _)| from == url && at.elapsed() < Duration::from_secs(600)) {
        return Ok(index);
    }
    let index: Arc<Index> = Arc::new(get_ok(client, url).await?.json().await?);
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((url.into(), Instant::now(), index.clone()));
    Ok(index)
}

impl Index {
    // ponytail: only content mods. A native mod is compiled against the exact game build with Horizon's
    // SDK and a Rust toolchain; the mod's API line is not compared either, the game refuses a wrong one.
    fn mods(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter().filter(|p| p.kind == "mod")
    }

    // The index lists a project's versions newest first.
    fn newest<'a>(&self, project: &'a Project, minecraft: &str) -> Option<&'a Version> {
        let revoked = |v: &Version| self.revoked.iter().any(|r| r.id == project.id && r.sha256.as_ref().is_none_or(|h| h.eq_ignore_ascii_case(&v.file.sha256)));
        project.versions.iter().find(|v| v.game == minecraft && !revoked(v))
    }
}

fn icon(project: &Project) -> Option<String> {
    project.icon.clone().filter(|i| crate::panel::picture(i).is_some())
}

pub async fn search(client: &Client, url: &str, instance: &Instance, query: &str, offset: usize) -> Result<Found> {
    let index = index(client, url).await?;
    let needle = query.to_lowercase();
    let matches = |p: &&Project| [&p.id, &p.name, &p.summary].iter().any(|text| text.to_lowercase().contains(&needle));
    let mut found: Vec<&Project> = index.mods().filter(|p| index.newest(p, &instance.minecraft).is_some()).filter(matches).collect();
    found.sort_by_key(|p| Reverse(p.downloads));
    let more = found.len() > offset + 20;
    let hit = |p: &Project| Hit { project_id: p.id.clone(), slug: p.id.clone(), title: p.name.clone(), description: p.summary.clone(), icon: icon(p), downloads: p.downloads, author: p.authors.join(", ") };
    Ok(Found { hits: found.into_iter().skip(offset).take(20).map(hit).collect(), more })
}

pub async fn installed(client: &Client, url: &str, paths: &Paths, instance: &Instance) -> Result<Vec<Installed>> {
    let (p, i) = (paths.clone(), instance.clone());
    let mut list: Vec<Installed> = blocking(move || {
        let dir = instance::game_dir(&p, &i.id).join(i.mods_place().0);
        instance::mods(&p, &i)?
            .into_iter()
            .map(|m| Ok(Installed { hash: sha256_file(&dir.join(&m.name))?, disabled: false, file: m.name, managed: m.managed, project: None, title: None, icon: None, update: false }))
            .collect()
    })
    .await?;
    if list.is_empty() {
        return Ok(list);
    }
    let index = index(client, url).await?;
    for file in &mut list {
        let made_it = |p: &&Project| p.versions.iter().any(|v| v.file.sha256.eq_ignore_ascii_case(&file.hash));
        if let Some(project) = index.mods().find(made_it) {
            file.update = !file.managed && index.newest(project, &instance.minecraft).is_some_and(|v| !v.file.sha256.eq_ignore_ascii_case(&file.hash));
            (file.project, file.title, file.icon) = (Some(project.id.clone()), Some(project.name.clone()), icon(project));
        }
    }
    Ok(list)
}

// A plain-http index is a test server; the real files are GitHub release downloads of the registry.
fn download_allowed(index: &str, url: &str) -> bool {
    url.starts_with(FILES) || (index.starts_with("http://") && url.starts_with("http://"))
}

pub async fn install(client: &Client, url: &str, paths: &Paths, instance: &Instance, project: &str, progress: &ProgressTx) -> Result<Outcome> {
    plain_id(project)?;
    let _lock = instance::lock(&instance.id)?;
    let instance = instance::load(paths, &instance.id)?;
    let index = index(client, url).await?;
    let found = index.mods().find(|p| p.id == project).ok_or_else(|| Error::Other(format!("{project} is not a mod in Horizon's registry")))?;
    let refuse = |reason: String| Ok(Outcome { installed: Vec::new(), failed: vec![Failure { name: found.name.clone(), reason }] });
    let Some(version) = index.newest(found, &instance.minecraft) else {
        return refuse(format!("no build for Horizon on Minecraft {}", instance.minecraft));
    };
    // The game knows a mod by its file name, so only <id>.hmod is taken.
    let name = format!("{project}.hmod");
    if safe_path(&version.file.name).ok().as_deref() != Some(name.as_str()) {
        return refuse(format!("Horizon's registry gave an unexpected file name ({})", version.file.name));
    }
    if !download_allowed(url, &version.file.url) {
        return refuse("its download is not from Horizon's registry".into());
    }
    let game = instance::game_dir(paths, &instance.id);
    let report = |done| drop(progress.send(Progress { step: 0, steps: 0, stage: "mods".into(), done, total: 1 }));
    report(0);
    let file = &version.file;
    fetch(client, &Download { url: file.url.clone(), path: game.join(instance.mods_place().0).join(&name), hash: Some(Hash::Sha256(file.sha256.clone())), size: file.size, bearer: None }).await?;
    blocking(move || crate::horizon::enable_mod(&game, &name[..name.len() - ".hmod".len()])).await?;
    report(1);
    Ok(Outcome { installed: vec![found.name.clone()], failed: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::{serve, serve_routes, tmp_dir};
    use crate::http_client;
    use crate::instance::{create_manual, game_dir};
    use crate::loader::Loader;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn world() -> (crate::download::testing::Tmp, Paths, Instance) {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        // The instance lock is global and keyed by id: every world needs its own.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!("horizon{}", NEXT.fetch_add(1, Ordering::SeqCst));
        let inst = create_manual(&paths, &name, "", Some(Loader::parse("horizon").unwrap())).unwrap();
        (t, paths, inst)
    }

    fn sha(bytes: &[u8]) -> String {
        let dir = tmp_dir();
        std::fs::write(dir.join("h"), bytes).unwrap();
        sha256_file(&dir.join("h")).unwrap()
    }

    fn version(id: &str, files: &str, bytes: &[u8], game: &str) -> Value {
        json!({"version": "1", "game": game, "api": "0.2", "file": {"name": format!("{id}.hmod"), "url": files, "size": bytes.len(), "sha256": sha(bytes)}})
    }

    fn project(id: &str, kind: &str, downloads: u64, versions: Value) -> Value {
        let icon = format!("https://horizon-rust.github.io/horizon-mods/icons/{id}.png?v=1");
        json!({"id": id, "kind": kind, "name": format!("Mod {id}"), "summary": "Lamps", "authors": ["a", "b"], "icon": icon, "downloads": downloads, "versions": versions})
    }

    async fn serve_index(projects: Value, revoked: Value) -> String {
        let body = json!({"schema": 1, "projects": projects, "revoked": revoked}).to_string().into_bytes();
        serve_routes(vec![("", 200, body, String::new())]).await.0
    }

    #[tokio::test]
    async fn a_search_offers_content_mods_for_this_minecraft_most_downloaded_first() {
        let (_t, _paths, inst) = world();
        let v = |id| json!([version(id, "http://x", b"x", "26.3")]);
        let old = json!([version("old", "http://x", b"x", "26.2")]);
        let projects = json!([project("small", "mod", 5, v("small")), project("native", "native-mod", 99, v("native")), project("big", "mod", 50, v("big")), project("old", "mod", 70, old), project("gone", "mod", 80, v("gone"))]);
        let url = serve_index(projects, json!([{"id": "gone", "reason": "taken down"}])).await;
        let found = search(&http_client(), &url, &inst, "", 0).await.unwrap();
        assert_eq!(found.hits.iter().map(|h| h.project_id.as_str()).collect::<Vec<_>>(), ["big", "small"]);
        let first = &found.hits[0];
        assert_eq!((first.title.as_str(), first.author.as_str(), first.downloads, found.more), ("Mod big", "a, b", 50, false));
        assert!(first.icon.as_deref().is_some_and(|i| i.ends_with("/icons/big.png?v=1")));
        assert_eq!(search(&http_client(), &url, &inst, "SMALL", 0).await.unwrap().hits.len(), 1);
        assert!(search(&http_client(), &url, &inst, "", 20).await.unwrap().hits.is_empty());
    }

    #[tokio::test]
    async fn an_install_writes_the_checked_file_turns_it_on_and_the_list_names_it() {
        let (_t, paths, inst) = world();
        let (files, _) = serve(200, b"the mod").await;
        let url = serve_index(json!([project("lights", "mod", 1, json!([version("lights", &files, b"the mod", "26.3")]))]), json!([])).await;
        let out = install(&http_client(), &url, &paths, &inst, "lights", &tokio::sync::mpsc::unbounded_channel().0).await.unwrap();
        assert_eq!((out.installed, out.failed.len()), (vec!["Mod lights".to_string()], 0));
        let game = game_dir(&paths, &inst.id);
        assert_eq!(std::fs::read(game.join("horizon-mods/lights.hmod")).unwrap(), b"the mod");
        assert_eq!(std::fs::read_to_string(game.join("horizon-mods.txt")).unwrap(), "mod.lights:true\n");
        std::fs::write(game.join("horizon-mods/hand.hmod"), b"by hand").unwrap();
        let list = installed(&http_client(), &url, &paths, &inst).await.unwrap();
        let view: Vec<_> = list.iter().map(|i| (i.file.as_str(), i.project.as_deref(), i.title.as_deref(), i.update)).collect();
        assert_eq!(view, [("hand.hmod", None, None, false), ("lights.hmod", Some("lights"), Some("Mod lights"), false)]);
    }

    #[tokio::test]
    async fn an_older_file_has_an_update_and_a_changed_download_is_refused() {
        let (_t, paths, inst) = world();
        let (files, _) = serve(200, b"not what the index pinned").await;
        let versions = json!([version("lights", &files, b"new build", "26.3"), version("lights", &files, b"old build", "26.3")]);
        let url = serve_index(json!([project("lights", "mod", 1, versions)]), json!([])).await;
        let mods = game_dir(&paths, &inst.id).join("horizon-mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("lights.hmod"), b"old build").unwrap();
        assert!(installed(&http_client(), &url, &paths, &inst).await.unwrap()[0].update);
        let err = install(&http_client(), &url, &paths, &inst, "lights", &tokio::sync::mpsc::unbounded_channel().0).await.unwrap_err();
        assert!(matches!(err, Error::Hash { .. }));
        assert_eq!(std::fs::read(mods.join("lights.hmod")).unwrap(), b"old build");
    }

    #[tokio::test]
    async fn an_odd_file_name_or_a_foreign_download_writes_nothing() {
        for (name, file_url) in [("../evil.hmod", "http://127.0.0.1:1/x"), ("other.hmod", "http://127.0.0.1:1/x"), ("lights.hmod", "https://evil.example/lights.hmod")] {
            let (_t, paths, inst) = world();
            let mut v = version("lights", file_url, b"x", "26.3");
            v["file"]["name"] = name.into();
            let url = serve_index(json!([project("lights", "mod", 1, json!([v]))]), json!([])).await;
            assert!(!download_allowed(INDEX, file_url), "{name}");
            let out = install(&http_client(), &url, &paths, &inst, "lights", &tokio::sync::mpsc::unbounded_channel().0).await.unwrap();
            assert_eq!((out.installed.len(), out.failed.len()), (0, 1), "{name}");
            assert!(!game_dir(&paths, &inst.id).join("horizon-mods.txt").exists(), "{name}");
        }
        assert!(download_allowed(INDEX, "https://github.com/horizon-rust/horizon-mods/releases/download/a-1/a.hmod"));
    }

    #[tokio::test]
    #[ignore = "reads Horizon's real registry; run by hand"]
    async fn the_real_index_is_read() {
        let (_t, _paths, inst) = world();
        let found = search(&http_client(), INDEX, &inst, "", 0).await.unwrap();
        assert!(found.more && found.hits.iter().all(|h| h.icon.is_some()), "{:?}", found.hits.first());
    }

    #[tokio::test]
    async fn a_native_mod_or_an_unknown_id_is_not_installed() {
        let (_t, paths, inst) = world();
        let url = serve_index(json!([project("native", "native-mod", 1, json!([version("native", "http://x", b"x", "26.3")]))]), json!([])).await;
        for id in ["native", "missing", "../x"] {
            assert!(install(&http_client(), &url, &paths, &inst, id, &tokio::sync::mpsc::unbounded_channel().0).await.is_err(), "{id}");
        }
    }
}
