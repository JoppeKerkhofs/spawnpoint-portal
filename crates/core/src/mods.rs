use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::download::{Download, Hash, Progress, ProgressTx, blocking, fetch, file_matches, get_ok, sha512_file};
use crate::instance::{self, Instance};
use crate::loader::Loader;
use crate::sync::safe_path;
use crate::{Error, Paths, Result};

pub const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub downloads: u64,
    pub author: String,
}

#[derive(Debug, Serialize)]
pub struct Found {
    pub hits: Vec<Hit>,
    pub more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub file: String,
    pub managed: bool,
    pub project: Option<String>,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub update: bool,
    pub disabled: bool,
    #[serde(skip)]
    hash: String,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Outcome {
    pub installed: Vec<String>,
    pub failed: Vec<Failure>,
}

#[derive(Deserialize)]
struct RawHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    downloads: u64,
    author: String,
}

#[derive(Deserialize)]
struct Search {
    hits: Vec<RawHit>,
    total_hits: usize,
}

#[derive(Deserialize)]
struct Dependency {
    project_id: Option<String>,
    version_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct Hashes {
    sha512: String,
}

#[derive(Deserialize)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    hashes: Hashes,
    size: Option<u64>,
}

#[derive(Deserialize)]
struct Version {
    project_id: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    files: Vec<VersionFile>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    title: String,
    icon_url: Option<String>,
}

#[derive(Deserialize)]
struct Known {
    project_id: String,
    date_published: String,
}

fn loader_of(instance: &Instance) -> Result<&Loader> {
    instance.loader.as_ref().ok_or_else(|| Error::Unsupported("This instance has no mod loader".into()))
}

fn plain_id(id: &str) -> Result<&str> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) {
        return Err(Error::Other(format!("{id:?} is not a Modrinth id")));
    }
    Ok(id)
}

async fn get<T: DeserializeOwned>(client: &Client, api: &str, path: &str, params: &[(&str, String)]) -> Result<T> {
    let mut url = reqwest::Url::parse(&format!("{api}{path}")).map_err(|e| Error::Other(e.to_string()))?;
    url.query_pairs_mut().extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
    Ok(get_ok(client, url.as_str()).await?.json().await?)
}

async fn post<T: DeserializeOwned>(client: &Client, api: &str, path: &str, body: &Value) -> Result<T> {
    let response = client.post(format!("{api}{path}")).json(body).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status { url: path.into(), status: status.as_u16() });
    }
    Ok(response.json().await?)
}

async fn projects(client: &Client, api: &str, ids: Vec<String>) -> Result<HashMap<String, Project>> {
    let ids: Vec<String> = ids.into_iter().collect::<HashSet<_>>().into_iter().collect();
    let mut out = HashMap::new();
    for chunk in ids.chunks(100) {
        let found: Vec<Project> = get(client, api, "/projects", &[("ids", json!(chunk).to_string())]).await?;
        out.extend(found.into_iter().map(|p| (p.id.clone(), p)));
    }
    Ok(out)
}

pub async fn search(client: &Client, api: &str, instance: &Instance, query: &str, offset: usize) -> Result<Found> {
    let loader = loader_of(instance)?.kind.as_str();
    let facets = json!([["project_type:mod"], [format!("categories:{loader}")], [format!("versions:{}", instance.minecraft)], ["client_side:required", "client_side:optional"]]);
    let params = [
        ("query", query.to_string()),
        ("facets", facets.to_string()),
        ("index", (if query.is_empty() { "downloads" } else { "relevance" }).into()),
        ("offset", offset.to_string()),
        ("limit", "20".into()),
    ];
    let found: Search = get(client, api, "/search", &params).await?;
    let more = offset + found.hits.len() < found.total_hits;
    let hits = found
        .hits
        .into_iter()
        .map(|h| Hit {
            icon: h.icon_url.filter(|i| crate::panel::picture(i).is_some()),
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            downloads: h.downloads,
            author: h.author,
        })
        .collect();
    Ok(Found { hits, more })
}

pub async fn installed(client: &Client, api: &str, paths: &Paths, instance: &Instance) -> Result<Vec<Installed>> {
    let loader = loader_of(instance)?.kind.as_str();
    let (p, i) = (paths.clone(), instance.clone());
    let mut list: Vec<Installed> = blocking(move || {
        let dir = instance::game_dir(&p, &i.id).join("mods");
        instance::mods(&p, &i)?
            .into_iter()
            .map(|m| Ok(Installed { hash: sha512_cached(&dir.join(&m.name))?, disabled: m.name.to_lowercase().ends_with(".disabled"), file: m.name, managed: m.managed, project: None, title: None, icon: None, update: false }))
            .collect()
    })
    .await?;
    if list.is_empty() {
        return Ok(list);
    }
    let hashes: Vec<&str> = list.iter().map(|j| j.hash.as_str()).collect();
    let known: HashMap<String, Known> = post(client, api, "/version_files", &json!({ "hashes": hashes, "algorithm": "sha512" })).await?;
    if known.is_empty() {
        return Ok(list);
    }
    let body = json!({ "hashes": hashes, "algorithm": "sha512", "loaders": [loader], "game_versions": [instance.minecraft], "version_types": ["release"] });
    let newer: HashMap<String, Known> = post(client, api, "/version_files/update", &body).await?;
    let names = projects(client, api, known.values().map(|k| k.project_id.clone()).collect()).await?;
    for jar in &mut list {
        if let Some(k) = known.get(&jar.hash) {
            let project = names.get(&k.project_id);
            jar.title = project.map(|p| p.title.clone());
            jar.icon = project.and_then(|p| p.icon_url.clone()).filter(|i| crate::panel::picture(i).is_some());
            jar.update = !jar.managed && !jar.disabled && newer.get(&jar.hash).is_some_and(|n| n.date_published > k.date_published);
            jar.project = Some(k.project_id.clone());
        }
    }
    Ok(list)
}

type Stamp = (PathBuf, u64, SystemTime);
// ponytail: never evicted, one entry per jar seen in a run; add eviction if that ever grows.
static HASHES: Mutex<Option<HashMap<Stamp, String>>> = Mutex::new(None);

fn sha512_cached(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let stamp = (path.to_path_buf(), meta.len(), meta.modified()?);
    if let Some(hash) = HASHES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().get(&stamp) {
        return Ok(hash.clone());
    }
    let hash = sha512_file(path)?;
    HASHES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().insert(stamp, hash.clone());
    Ok(hash)
}

// File names on Windows ignore case.
fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn jar_name(name: &str) -> std::result::Result<String, String> {
    safe_path(name)
        .ok()
        .filter(|n| !n.contains('/') && n.to_lowercase().ends_with(".jar"))
        .ok_or_else(|| format!("Modrinth gave an unsafe file name ({name})"))
}

// A plain-http API is a test server; the real one is https.
fn download_allowed(api: &str, url: &str) -> bool {
    url.starts_with("https://cdn.modrinth.com/") || (api.starts_with("http://") && url.starts_with("http://"))
}

async fn remove_if_unchanged(path: &Path, hash: &str) -> Result<bool> {
    let (path, hash) = (path.to_path_buf(), Hash::Sha512(hash.to_string()));
    blocking(move || {
        if path.exists() && !file_matches(&path, &hash) {
            return Ok(false);
        }
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(true),
        }
    })
    .await
}

struct Put {
    version: Version,
    kept: Vec<Failure>,
}

// Ok(Err(reason)) is a mod that cannot be installed; Err is a failure of the install itself.
async fn put(client: &Client, api: &str, mods: &Path, target: (&str, &str), have: &[&Installed], project: &str) -> Result<std::result::Result<Put, String>> {
    let (loader, minecraft) = target;
    if have.iter().any(|j| j.managed) {
        return Ok(Err("it comes from the pack, and the sync manages it".into()));
    }
    if !have.is_empty() && have.iter().all(|j| j.disabled) {
        return Ok(Err(format!("{} is in this instance but disabled.", have[0].title.as_deref().unwrap_or("It"))));
    }
    let params = [("loaders", json!([loader]).to_string()), ("game_versions", json!([minecraft]).to_string()), ("include_changelog", "false".into())];
    let versions: Vec<Version> = get(client, api, &format!("/project/{project}/version"), &params).await?;
    let pick = versions.iter().position(|v| v.version_type == "release").unwrap_or(0);
    let Some(version) = versions.into_iter().nth(pick) else {
        return Ok(Err(format!("no build for {loader} on Minecraft {minecraft}")));
    };
    let Some(file) = version.files.iter().find(|f| f.primary).or(version.files.first()) else {
        return Ok(Err("its newest build has no file".into()));
    };
    let name = match jar_name(&file.filename) {
        Ok(n) => n,
        Err(reason) => return Ok(Err(reason)),
    };
    if !download_allowed(api, &file.url) {
        return Ok(Err("its download is not from Modrinth".into()));
    }
    let dest = mods.join(&name);
    let ours = have.iter().find(|j| same_name(&j.file, &name)).map(|j| (dest.clone(), Hash::Sha512(j.hash.clone())));
    let free = !dest.exists()
        || match ours {
            Some((path, hash)) => blocking(move || Ok(file_matches(&path, &hash))).await?,
            None => false,
        };
    if !free {
        return Ok(Err(format!("{name} is already in this instance")));
    }
    let item = Download { url: file.url.clone(), path: dest, hash: Some(Hash::Sha512(file.hashes.sha512.clone())), size: file.size, bearer: None };
    fetch(client, &item).await?;
    let mut kept = Vec::new();
    for old in have.iter().filter(|j| !j.disabled && !same_name(&j.file, &name)) {
        let reason = match remove_if_unchanged(&mods.join(&old.file), &old.hash).await {
            Ok(true) => continue,
            Ok(false) => "you changed it meanwhile, so it was left in place".to_string(),
            Err(_) => "remove it yourself".to_string(),
        };
        kept.push(Failure { name: old.file.clone(), reason: format!("The old file {} could not be removed: {reason}.", old.file) });
    }
    Ok(Ok(Put { version, kept }))
}

async fn version_project(client: &Client, api: &str, version: &str) -> Result<String> {
    Ok(get::<Version>(client, api, &format!("/version/{}", plain_id(version)?), &[]).await?.project_id)
}

pub async fn install(client: &Client, api: &str, paths: &Paths, instance: &Instance, project: &str, progress: &ProgressTx) -> Result<Outcome> {
    plain_id(project)?;
    let _lock = instance::lock(&instance.id)?;
    let instance = instance::load(paths, &instance.id)?;
    let target = (loader_of(&instance)?.kind.as_str(), instance.minecraft.as_str());
    let have = installed(client, api, paths, &instance).await?;
    let mods = instance::game_dir(paths, &instance.id).join("mods");
    let mut queue = VecDeque::from([project.to_string()]);
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    let mut handled = 0;
    let report = |done: u64, total: u64| {
        let _ = progress.send(Progress { step: 0, steps: 0, stage: "mods".into(), done, total });
    };
    while let Some(next) = queue.pop_front() {
        let own: Vec<&Installed> = have.iter().filter(|j| j.project.as_deref() == Some(&next)).collect();
        if next != project && !own.is_empty() {
            continue;
        }
        report(handled, handled + queue.len() as u64 + 1);
        handled += 1;
        let put = match put(client, api, &mods, target, &own, &next).await {
            Err(e) if next == project => return Err(e),
            Err(e) => Err(e.to_string()),
            Ok(r) => r,
        };
        let Put { version, kept } = match put {
            Ok(p) => p,
            Err(reason) => {
                failed.push(Failure { name: next, reason });
                continue;
            }
        };
        failed.extend(kept);
        done.push(version.project_id.clone());
        for d in version.dependencies.iter().filter(|d| d.dependency_type == "required") {
            let id = match (&d.project_id, &d.version_id) {
                (Some(p), _) => Some(p.clone()),
                (None, Some(v)) => match version_project(client, api, v).await {
                    Ok(p) => Some(p),
                    Err(e) => {
                        failed.push(Failure { name: v.clone(), reason: e.to_string() });
                        None
                    }
                },
                _ => None,
            };
            if let Some(id) = id.filter(|id| plain_id(id).is_ok() && seen.insert(id.clone())) {
                queue.push_back(id);
            }
        }
    }
    report(handled, handled);
    let ids = done.iter().chain(failed.iter().map(|f| &f.name)).cloned().collect();
    let names = projects(client, api, ids).await.unwrap_or_default();
    let title = |id: &String| names.get(id).map_or(id, |p| &p.title).clone();
    Ok(Outcome {
        installed: done.iter().map(title).collect(),
        failed: failed.iter().map(|f| Failure { name: title(&f.name), reason: f.reason.clone() }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::{Tmp, serve, serve_routes, tmp_dir};
    use crate::http_client;
    use crate::instance::{ManagedFile, create_manual, game_dir};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SEARCH: &str = include_str!("../tests/fixtures/modrinth-search.json");
    const SODIUM: &str = include_str!("../tests/fixtures/modrinth-versions-sodium.json");
    const IRIS: &str = include_str!("../tests/fixtures/modrinth-versions-iris.json");
    const KNOWN: &str = include_str!("../tests/fixtures/modrinth-version-files.json");
    const PROJECTS: &str = include_str!("../tests/fixtures/modrinth-projects.json");
    const OLD: &str = "2026-07-06T17:19:52.471697Z";
    const NEW: &str = "2026-08-28T20:55:02.424329Z";

    type Route = (&'static str, u16, Vec<u8>, String);
    type Heads = Arc<std::sync::Mutex<Vec<String>>>;

    struct World {
        _t: Tmp,
        paths: Paths,
        inst: Instance,
    }

    fn world(loader: Option<&str>) -> World {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        // The instance lock is global and keyed by id: every world needs its own.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!("mods{}", NEXT.fetch_add(1, Ordering::SeqCst));
        let inst = create_manual(&paths, &name, "1.21.1", loader.map(|l| Loader::parse(l).unwrap())).unwrap();
        World { _t: t, paths, inst }
    }

    impl World {
        fn mods(&self) -> std::path::PathBuf {
            game_dir(&self.paths, &self.inst.id).join("mods")
        }
        fn put(&self, name: &str, bytes: &[u8]) -> String {
            std::fs::create_dir_all(self.mods()).unwrap();
            std::fs::write(self.mods().join(name), bytes).unwrap();
            sha(bytes)
        }
        fn read(&self, name: &str) -> Option<Vec<u8>> {
            std::fs::read(self.mods().join(name)).ok()
        }
    }

    fn sha(bytes: &[u8]) -> String {
        let dir = tmp_dir();
        std::fs::write(dir.join("h"), bytes).unwrap();
        sha512_file(&dir.join("h")).unwrap()
    }

    fn json_route(key: &'static str, body: &str) -> Route {
        (key, 200, body.as_bytes().to_vec(), String::new())
    }

    // One real sodium version, pointed at a test file.
    fn version(project: &str, name: &str, url: &str, file: &[u8], deps: Value) -> Value {
        let mut v = serde_json::from_str::<Vec<Value>>(SODIUM).unwrap().remove(0);
        v["project_id"] = project.into();
        v["files"][0]["url"] = url.into();
        v["files"][0]["filename"] = name.into();
        v["files"][0]["hashes"]["sha512"] = sha(file).into();
        v["dependencies"] = deps;
        v
    }

    // The real iris versions; the first one points at a test file.
    fn iris(url: &str, file: &[u8], deps: Option<Value>) -> String {
        let mut v: Vec<Value> = serde_json::from_str(IRIS).unwrap();
        v[0]["files"][0]["url"] = url.into();
        v[0]["files"][0]["hashes"]["sha512"] = sha(file).into();
        if let Some(d) = deps {
            v[0]["dependencies"] = d;
        }
        json!(v).to_string()
    }

    fn dep(project: &str, kind: &str) -> Value {
        json!({"project_id": project, "version_id": null, "file_name": null, "dependency_type": kind})
    }

    // The real answer's shape, keyed by the hashes of the test files.
    fn known(entries: &[(&str, &str, &str)]) -> String {
        let all: Value = serde_json::from_str(KNOWN).unwrap();
        let tpl = all.as_object().unwrap().values().next().unwrap().clone();
        let map: serde_json::Map<String, Value> = entries
            .iter()
            .map(|(hash, project, date)| {
                let mut v = tpl.clone();
                v["project_id"] = (*project).into();
                v["date_published"] = (*date).into();
                (hash.to_string(), v)
            })
            .collect();
        Value::Object(map).to_string()
    }

    fn requests(heads: &Heads, part: &str) -> usize {
        heads.lock().unwrap().iter().filter(|h| h.lines().next().unwrap_or("").contains(part)).count()
    }

    fn base(known: &str, versions: &str) -> Vec<Route> {
        vec![
            json_route("POST /version_files/update", "{}"),
            json_route("POST /version_files", known),
            json_route("GET /projects", PROJECTS),
            json_route("GET /project/AANobbMI/version", versions),
        ]
    }

    async fn api(routes: Vec<Route>) -> (String, Arc<AtomicUsize>, Heads) {
        serve_routes(routes).await
    }

    fn quiet() -> ProgressTx {
        tokio::sync::mpsc::unbounded_channel().0
    }

    #[tokio::test]
    async fn an_install_reports_its_files_ending_with_done_equal_to_total() {
        let w = world(Some("fabric:0.1"));
        let (iris_files, _) = serve(200, b"iris jar").await;
        let (sodium_files, _) = serve(200, b"sodium jar").await;
        let deps = json!([dep("AANobbMI", "required")]);
        let sodium = version("AANobbMI", "sodium.jar", &sodium_files, b"sodium jar", json!([]));
        let mut routes = base("{}", &json!([sodium]).to_string());
        routes.push(json_route("GET /project/YL57xq9U/version", &iris(&iris_files, b"iris jar", Some(deps))));
        let (url, _, _) = api(routes).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        install(&http_client(), &url, &w.paths, &w.inst, "YL57xq9U", &tx).await.unwrap();
        let mut events = Vec::new();
        while let Ok(p) = rx.try_recv() {
            events.push(p);
        }
        let last = events.last().unwrap();
        assert!(events.iter().all(|p| p.stage == "mods"));
        assert_eq!((last.done, last.total), (2, 2));
    }

    #[tokio::test]
    async fn a_search_asks_for_this_loader_and_version_and_reads_the_answer() {
        let w = world(Some("fabric:0.1"));
        let (url, _, heads) = api(vec![json_route("GET /search", SEARCH)]).await;
        let found = search(&http_client(), &url, &w.inst, "sodium", 0).await.unwrap();
        assert_eq!((found.hits.len(), found.more), (3, true));
        let first = &found.hits[0];
        assert_eq!(
            (first.project_id.as_str(), first.slug.as_str(), first.title.as_str(), first.author.as_str()),
            ("AANobbMI", "sodium", "Sodium", "jellysquid3")
        );
        assert!(first.icon.as_deref().is_some_and(|i| i.starts_with("https://cdn.modrinth.com/")));
        assert!(first.downloads > 1000);
        let line = heads.lock().unwrap()[0].lines().next().unwrap().to_string();
        for part in ["index=relevance", "limit=20", "offset=0", "query=sodium", "categories%3Afabric", "versions%3A1.21.1", "project_type%3Amod", "client_side%3Aoptional"] {
            assert!(line.contains(part), "{part} in {line}");
        }
        search(&http_client(), &url, &w.inst, "", 20).await.unwrap();
        let line = heads.lock().unwrap()[1].lines().next().unwrap().to_string();
        assert!(line.contains("index=downloads") && line.contains("offset=20"), "{line}");
    }

    #[tokio::test]
    async fn an_icon_off_the_cdn_is_dropped_and_the_last_page_says_so() {
        let w = world(Some("fabric:0.1"));
        let mut body: Value = serde_json::from_str(SEARCH).unwrap();
        body["hits"][0]["icon_url"] = "https://evil.example/x.png".into();
        body["hits"][1]["icon_url"] = Value::Null;
        body["total_hits"] = 3.into();
        let (url, _, _) = api(vec![json_route("GET /search", &body.to_string())]).await;
        let found = search(&http_client(), &url, &w.inst, "x", 0).await.unwrap();
        assert_eq!((found.hits[0].icon.clone(), found.hits[1].icon.clone(), found.more), (None, None, false));
        assert!(found.hits[2].icon.is_some());
    }

    #[tokio::test]
    async fn an_instance_without_a_loader_cannot_have_mods() {
        let w = world(None);
        let (url, hits, _) = api(vec![]).await;
        let c = http_client();
        for err in [
            search(&c, &url, &w.inst, "", 0).await.err().unwrap(),
            installed(&c, &url, &w.paths, &w.inst).await.err().unwrap(),
            install(&c, &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.err().unwrap(),
        ] {
            assert!(matches!(&err, Error::Unsupported(m) if m == "This instance has no mod loader"));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn the_installed_list_names_what_modrinth_knows_and_what_has_a_newer_build() {
        let w = world(Some("fabric:0.1"));
        let own = w.put("mine.jar", b"own sodium");
        let pack = w.put("pack.jar", b"pack iris");
        w.put("hand.jar", b"by hand");
        w.put("off.jar.disabled", b"off");
        let mut inst = w.inst.clone();
        inst.managed.push(ManagedFile { path: "mods/pack.jar".into(), sha1: "x".into() });
        let (url, _, _) = api(vec![
            json_route("POST /version_files/update", &known(&[(&own, "AANobbMI", NEW), (&pack, "YL57xq9U", NEW)])),
            json_route("POST /version_files", &known(&[(&own, "AANobbMI", OLD), (&pack, "YL57xq9U", OLD)])),
            json_route("GET /projects", PROJECTS),
        ])
        .await;
        let list = installed(&http_client(), &url, &w.paths, &inst).await.unwrap();
        let icons: Vec<_> = list.iter().map(|i| i.icon.as_deref().map(|u| u.starts_with("https://cdn.modrinth.com/data/"))).collect();
        assert_eq!(icons, [None, Some(true), None, Some(true)]);
        let view: Vec<_> = list.iter().map(|i| (i.file.as_str(), i.managed, i.project.as_deref(), i.title.as_deref(), i.update)).collect();
        assert_eq!(
            view,
            [
                ("hand.jar", false, None, None, false),
                ("mine.jar", false, Some("AANobbMI"), Some("Sodium"), true),
                ("off.jar.disabled", false, None, None, false),
                ("pack.jar", true, Some("YL57xq9U"), Some("Iris Shaders"), false),
            ]
        );
    }

    #[tokio::test]
    async fn an_empty_mods_folder_asks_nobody() {
        let w = world(Some("fabric:0.1"));
        let (url, hits, _) = api(vec![]).await;
        assert!(installed(&http_client(), &url, &w.paths, &w.inst).await.unwrap().is_empty());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_same_named_file_of_the_player_is_left_alone() {
        let w = world(Some("fabric:0.1"));
        w.put("sodium-new.jar", b"the player's own file");
        let (files, file_hits) = serve(200, b"modrinth's jar").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"modrinth's jar", json!([]));
        let (url, _, _) = api(base("{}", &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert!(out.installed.is_empty());
        assert_eq!(out.failed.len(), 1);
        assert!(out.failed[0].reason.contains("sodium-new.jar is already in this instance"), "{}", out.failed[0].reason);
        assert_eq!(w.read("sodium-new.jar").unwrap(), b"the player's own file");
        assert_eq!(file_hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_jar_of_the_pack_is_never_replaced_or_removed() {
        let w = world(Some("fabric:0.1"));
        let hash = w.put("pack-sodium.jar", b"the pack's sodium");
        let mut inst = w.inst.clone();
        inst.managed.push(ManagedFile { path: "mods/pack-sodium.jar".into(), sha1: "x".into() });
        instance::save(&w.paths, &inst).unwrap();
        let (files, file_hits) = serve(200, b"modrinth's jar").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"modrinth's jar", json!([]));
        let (url, _, _) = api(base(&known(&[(&hash, "AANobbMI", OLD)]), &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &inst, "AANobbMI", &quiet()).await.unwrap();
        assert!(out.installed.is_empty());
        assert!(out.failed[0].reason.contains("pack"), "{}", out.failed[0].reason);
        assert_eq!(w.read("pack-sodium.jar").unwrap(), b"the pack's sodium");
        assert_eq!(w.read("sodium-new.jar"), None);
        assert_eq!(file_hits.load(Ordering::SeqCst), 0);
        assert_eq!(instance::load(&w.paths, &inst.id).unwrap().managed.len(), 1);
    }

    #[tokio::test]
    async fn an_own_old_jar_goes_after_the_new_one_is_written_and_the_new_one_stays_the_players() {
        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium-old.jar", b"old build");
        w.put("other.jar", b"another mod");
        let (files, _) = serve(200, b"new build").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"new build", json!([]));
        let (url, _, _) = api(base(&known(&[(&hash, "AANobbMI", OLD)]), &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert_eq!((out.installed, out.failed.len()), (vec!["Sodium".to_string()], 0));
        assert_eq!(w.read("sodium-new.jar").unwrap(), b"new build");
        assert_eq!(w.read("sodium-old.jar"), None);
        assert_eq!(w.read("other.jar").unwrap(), b"another mod");
        assert!(instance::load(&w.paths, &w.inst.id).unwrap().managed.is_empty());
        assert!(!w.mods().join("sodium-new.jar.portal-part").exists());
    }

    #[tokio::test]
    async fn a_failed_download_leaves_the_old_jar_in_place() {
        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium-old.jar", b"old build");
        let (files, _) = serve(404, b"").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"new build", json!([]));
        let (url, _, _) = api(base(&known(&[(&hash, "AANobbMI", OLD)]), &json!([v]).to_string())).await;
        let err = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap_err();
        assert!(matches!(err, Error::Status { status: 404, .. }));
        assert_eq!(w.read("sodium-old.jar").unwrap(), b"old build");
        assert_eq!(w.read("sodium-new.jar"), None);
        assert!(!w.mods().join("sodium-new.jar.portal-part").exists());
    }

    #[tokio::test]
    async fn an_old_jar_the_player_changed_meanwhile_is_not_removed() {
        let dir = tmp_dir();
        let path = dir.join("a.jar");
        std::fs::write(&path, b"changed").unwrap();
        assert!(!remove_if_unchanged(&path, &sha(b"as scanned")).await.unwrap());
        assert!(path.exists());
        assert!(remove_if_unchanged(&path, &sha(b"changed")).await.unwrap());
        assert!(!path.exists());
        assert!(remove_if_unchanged(&path, &sha(b"changed")).await.unwrap());
    }

    #[tokio::test]
    async fn required_dependencies_come_along_once_and_cycles_stop() {
        let w = world(Some("fabric:0.1"));
        let (iris_files, _) = serve(200, b"iris jar").await;
        let (sodium_files, _) = serve(200, b"sodium jar").await;
        let deps = json!([dep("AANobbMI", "required"), dep("AANobbMI", "required"), dep("NOTNEEDED", "optional")]);
        let sodium = version("AANobbMI", "sodium.jar", &sodium_files, b"sodium jar", json!([dep("YL57xq9U", "required")]));
        let mut routes = base("{}", &json!([sodium]).to_string());
        routes.push(json_route("GET /project/YL57xq9U/version", &iris(&iris_files, b"iris jar", Some(deps))));
        let (url, _, heads) = api(routes).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "YL57xq9U", &quiet()).await.unwrap();
        assert_eq!((out.installed, out.failed.len()), (vec!["Iris Shaders".to_string(), "Sodium".to_string()], 0));
        assert_eq!(w.read("sodium.jar").unwrap(), b"sodium jar");
        assert_eq!(requests(&heads, "/project/AANobbMI/version"), 1);
        assert_eq!(requests(&heads, "/project/YL57xq9U/version"), 1);
        assert_eq!(requests(&heads, "NOTNEEDED"), 0);
    }

    #[tokio::test]
    async fn a_dependency_that_is_already_there_is_not_installed_again() {
        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium-mine.jar", b"sodium");
        let (iris_files, _) = serve(200, b"iris jar").await;
        let mut routes = base(&known(&[(&hash, "AANobbMI", OLD)]), "[]");
        routes.push(json_route("GET /project/YL57xq9U/version", &iris(&iris_files, b"iris jar", None)));
        let (url, _, heads) = api(routes).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "YL57xq9U", &quiet()).await.unwrap();
        assert_eq!(out.installed, ["Iris Shaders"]);
        assert_eq!(requests(&heads, "/project/AANobbMI/version"), 0);
    }

    #[tokio::test]
    async fn a_dependency_named_only_by_its_version_is_found_through_that_version() {
        let w = world(Some("fabric:0.1"));
        let (iris_files, _) = serve(200, b"iris jar").await;
        let (sodium_files, _) = serve(200, b"sodium jar").await;
        let only_version = json!([{"project_id": null, "version_id": "SMxNOGZ6", "file_name": null, "dependency_type": "required"}]);
        let sodium = version("AANobbMI", "sodium.jar", &sodium_files, b"sodium jar", json!([]));
        let mut routes = base("{}", &json!([sodium.clone()]).to_string());
        routes.push(json_route("GET /version/SMxNOGZ6", &sodium.to_string()));
        routes.push(json_route("GET /project/YL57xq9U/version", &iris(&iris_files, b"iris jar", Some(only_version))));
        let (url, _, _) = api(routes).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "YL57xq9U", &quiet()).await.unwrap();
        assert_eq!(out.installed, ["Iris Shaders", "Sodium"]);
    }

    #[tokio::test]
    async fn an_unsafe_file_name_from_modrinth_writes_nothing() {
        for name in ["../evil.jar", "evil.txt", "a\\b.jar", "C:evil.jar", "mods/x.jar", "aux.jar"] {
            let w = world(Some("fabric:0.1"));
            let (files, file_hits) = serve(200, b"x").await;
            let v = version("AANobbMI", name, &files, b"x", json!([]));
            let (url, _, _) = api(base("{}", &json!([v]).to_string())).await;
            let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
            assert!(out.installed.is_empty() && out.failed.len() == 1, "{name}");
            assert_eq!(file_hits.load(Ordering::SeqCst), 0, "{name}");
            assert!(!w.mods().exists() || std::fs::read_dir(w.mods()).unwrap().count() == 0, "{name}");
            assert!(!w.mods().parent().unwrap().join("evil.jar").exists(), "{name}");
        }
    }

    #[test]
    fn a_download_must_come_from_modrinth_unless_the_api_itself_is_plain_http() {
        assert!(download_allowed(API, "https://cdn.modrinth.com/a.jar"));
        assert!(!download_allowed(API, "https://evil.example/a.jar"));
        assert!(!download_allowed(API, "https://cdn.modrinth.com.evil.example/a.jar"));
        assert!(!download_allowed(API, "http://cdn.modrinth.com/a.jar"));
        assert!(!download_allowed(API, "file:///a.jar"));
        assert!(download_allowed("http://127.0.0.1:1", "http://127.0.0.1:2/a.jar"));
    }

    #[tokio::test]
    async fn a_download_from_another_https_host_is_refused_before_any_fetch() {
        let w = world(Some("fabric:0.1"));
        let v = version("AANobbMI", "sodium.jar", "https://evil.example/sodium.jar", b"x", json!([]));
        let (url, _, heads) = api(base("{}", &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert!(out.installed.is_empty() && out.failed.len() == 1);
        assert_eq!(requests(&heads, "evil.example"), 0);
        assert_eq!(w.read("sodium.jar"), None);
    }

    #[tokio::test]
    async fn a_disabled_jar_is_seen_and_never_touched() {
        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium.jar.disabled", b"switched off");
        let (files, file_hits) = serve(200, b"modrinth's jar").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"modrinth's jar", json!([]));
        let (url, _, heads) = api(base(&known(&[(&hash, "AANobbMI", OLD)]), &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert!(out.installed.is_empty());
        assert_eq!(out.failed[0].reason, "Sodium is in this instance but disabled.");
        let seen = installed(&http_client(), &url, &w.paths, &w.inst).await.unwrap();
        assert_eq!(seen.iter().map(|j| (j.file.as_str(), j.disabled)).collect::<Vec<_>>(), [("sodium.jar.disabled", true)]);
        assert_eq!((w.read("sodium.jar.disabled").unwrap(), w.read("sodium-new.jar")), (b"switched off".to_vec(), None));
        assert_eq!((file_hits.load(Ordering::SeqCst), requests(&heads, "/project/AANobbMI/version")), (0, 0));

        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium.jar.disabled", b"switched off");
        let (iris_files, _) = serve(200, b"iris jar").await;
        let mut routes = base(&known(&[(&hash, "AANobbMI", OLD)]), "[]");
        routes.push(json_route("GET /project/YL57xq9U/version", &iris(&iris_files, b"iris jar", Some(json!([dep("AANobbMI", "required")])))));
        let (url, _, heads) = api(routes).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "YL57xq9U", &quiet()).await.unwrap();
        assert_eq!(out.installed, ["Iris Shaders"]);
        assert_eq!(requests(&heads, "/project/AANobbMI/version"), 0);
        assert_eq!(w.read("sodium.jar.disabled").unwrap(), b"switched off");
    }

    #[test]
    fn an_unchanged_jar_is_hashed_once_and_a_changed_one_again() {
        let dir = tmp_dir();
        let path = dir.join("a.jar");
        std::fs::write(&path, b"aaaa").unwrap();
        let first = sha512_cached(&path).unwrap();
        let time = std::fs::metadata(&path).unwrap().modified().unwrap();
        let set = |bytes: &[u8], time| {
            std::fs::write(&path, bytes).unwrap();
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(time).unwrap();
        };
        set(b"bbbb", time);
        assert_eq!(sha512_cached(&path).unwrap(), first, "same size and time: not read again");
        set(b"bbbbb", time);
        assert_eq!(sha512_cached(&path).unwrap(), sha(b"bbbbb"));
        set(b"cccccc", time + std::time::Duration::from_secs(5));
        assert_eq!(sha512_cached(&path).unwrap(), sha(b"cccccc"));
        set(b"dddddd", time + std::time::Duration::from_secs(9));
        assert_eq!(sha512_cached(&path).unwrap(), sha(b"dddddd"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn an_old_jar_that_cannot_be_removed_is_reported_and_the_install_is_done() {
        use std::os::windows::fs::OpenOptionsExt;
        let w = world(Some("fabric:0.1"));
        let hash = w.put("sodium-old.jar", b"old build");
        let _held = std::fs::OpenOptions::new().read(true).share_mode(1).open(w.mods().join("sodium-old.jar")).unwrap();
        let (files, _) = serve(200, b"new build").await;
        let v = version("AANobbMI", "sodium-new.jar", &files, b"new build", json!([]));
        let (url, _, _) = api(base(&known(&[(&hash, "AANobbMI", OLD)]), &json!([v]).to_string())).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert_eq!(out.installed, ["Sodium"]);
        assert_eq!(out.failed.len(), 1);
        assert_eq!(out.failed[0].reason, "The old file sodium-old.jar could not be removed: remove it yourself.");
        assert_eq!(w.read("sodium-new.jar").unwrap(), b"new build");
        assert_eq!(w.read("sodium-old.jar").unwrap(), b"old build");
    }

    #[tokio::test]
    async fn a_mod_with_no_build_for_this_loader_is_reported() {
        let w = world(Some("fabric:0.1"));
        let (url, _, _) = api(base("{}", "[]")).await;
        let out = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap();
        assert!(out.installed.is_empty());
        assert_eq!((out.failed[0].name.as_str(), out.failed[0].reason.as_str()), ("Sodium", "no build for fabric on Minecraft 1.21.1"));
    }

    #[tokio::test]
    async fn a_bad_project_id_fails_before_a_request_and_a_held_lock_refuses() {
        let w = world(Some("fabric:0.1"));
        let (url, hits, _) = api(vec![]).await;
        for bad in ["", "../x", "a/b", "a.b", "a b", "a?x"] {
            assert!(install(&http_client(), &url, &w.paths, &w.inst, bad, &quiet()).await.is_err(), "{bad:?}");
        }
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        let _held = instance::lock(&w.inst.id).unwrap();
        let err = install(&http_client(), &url, &w.paths, &w.inst, "AANobbMI", &quiet()).await.unwrap_err();
        assert!(matches!(err, Error::Busy(_)));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }
}
