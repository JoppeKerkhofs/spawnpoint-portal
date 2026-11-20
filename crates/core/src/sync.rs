use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::download::{Download, Hash, ProgressTx, fetch_all, file_matches, part_path, sha1_bytes, sha1_file};
use crate::instance::{self, Instance, ManagedFile};
use crate::paths::check_relative;
use crate::{Error, Paths, Result};

#[derive(Debug, Clone)]
pub struct Wanted {
    pub path: String,
    pub from: Origin,
    pub hash: Option<Hash>,
    pub size: Option<u64>,
}

#[derive(Clone)]
pub enum Origin {
    Url(String),
    Bytes(Vec<u8>),
    Manual { name: String, page: String },
    // Build it with `panel::panel_file`, which ties the bearer to the panel's own address.
    PanelFile { url: String, bearer: String },
}

impl std::fmt::Debug for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Origin::Url(u) => f.debug_tuple("Url").field(u).finish(),
            Origin::Bytes(b) => f.debug_tuple("Bytes").field(b).finish(),
            Origin::Manual { name, page } => f.debug_struct("Manual").field("name", name).field("page", page).finish(),
            Origin::PanelFile { url, .. } => f.debug_struct("PanelFile").field("url", url).field("bearer", &"***").finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    pub path: String,
    pub name: String,
    pub page: String,
}

#[derive(Debug, Clone, Default)]
pub struct SyncPlan {
    pub write: Vec<Wanted>,
    pub delete: Vec<ManagedFile>,
    pub managed: Vec<ManagedFile>,
    pub manual: Vec<Manual>,
    // SHA-1 of the file that was at each write target when planned; absent = nothing was there
    pub replaces: HashMap<String, String>,
    // wanted paths left alone because the player's own file or folder is there
    pub skipped: Vec<String>,
}

const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

fn is_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = |p: &str| stem.strip_prefix(p).is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']));
    RESERVED.contains(&stem.as_str()) || numbered("COM") || numbered("LPT")
}

pub fn safe_path(path: &str) -> Result<String> {
    let norm = path.replace('\\', "/");
    let norm = norm.strip_prefix("./").unwrap_or(&norm);
    let bad_segment = norm.split('/').any(|s| {
        s.is_empty() || s == "." || s == ".." || s.ends_with(['.', ' ']) || is_device_name(s)
    });
    if norm.contains(':') || bad_segment || check_relative(norm).is_err() {
        return Err(Error::Other(format!("unsafe path in pack: {path}")));
    }
    Ok(norm.to_string())
}

pub fn plan(game_dir: &Path, managed: &[ManagedFile], wanted: Vec<Wanted>) -> Result<SyncPlan> {
    let mut slots: Vec<Option<Wanted>> = Vec::new();
    let mut slot_of: HashMap<String, usize> = HashMap::new();
    for mut w in wanted {
        w.path = safe_path(&w.path)?;
        if let (Origin::Bytes(bytes), None) = (&w.from, &w.hash) {
            w.hash = Some(Hash::Sha1(sha1_bytes(bytes)));
        }
        if let Some(old) = slot_of.insert(w.path.to_lowercase(), slots.len()) {
            slots[old] = None;
        }
        slots.push(Some(w));
    }
    let list: Vec<Wanted> = slots.into_iter().flatten().collect();
    let mut managed_by_path: HashMap<String, &str> = HashMap::new();
    for m in managed {
        managed_by_path.entry(m.path.to_lowercase()).or_insert(m.sha1.as_str());
    }
    let managed_sha = |p: &str| managed_by_path.get(&p.to_lowercase()).copied();
    let mut out = SyncPlan {
        write: vec![],
        delete: vec![],
        managed: vec![],
        manual: vec![],
        replaces: HashMap::new(),
        skipped: vec![],
    };

    for w in list {
        let full = game_dir.join(&w.path);
        if !full.exists() {
            match w.from {
                Origin::Manual { name, page } => out.manual.push(Manual { path: w.path, name, page }),
                _ => out.write.push(w),
            }
            continue;
        }
        if !full.is_file() {
            if !matches!(w.from, Origin::Manual { .. }) {
                out.skipped.push(w.path);
            }
            continue;
        }
        let sha1 = sha1_file(&full)?;
        let ours = managed_sha(&w.path).is_some_and(|m| m.eq_ignore_ascii_case(&sha1));
        let right = match &w.hash {
            Some(Hash::Sha1(h)) => h.eq_ignore_ascii_case(&sha1),
            Some(h) => file_matches(&full, h),
            None => false,
        };
        let manual = matches!(w.from, Origin::Manual { .. });
        if right || (ours && (manual || w.hash.is_none())) {
            out.managed.push(ManagedFile { path: w.path, sha1 });
        } else if ours {
            out.replaces.insert(w.path.clone(), sha1);
            out.write.push(w);
        } else if !manual {
            out.skipped.push(w.path);
        }
    }

    for m in managed {
        let Ok(p) = safe_path(&m.path) else { continue };
        let full = game_dir.join(&p);
        if !slot_of.contains_key(&p.to_lowercase())
            && full.is_file()
            && sha1_file(&full)?.eq_ignore_ascii_case(&m.sha1)
        {
            out.delete.push(ManagedFile { path: p, sha1: m.sha1.clone() });
        }
    }
    Ok(out)
}

fn still_ours(file: &Path, w: &Wanted, replaces: &HashMap<String, String>) -> bool {
    if !file.exists() {
        return true;
    }
    let Ok(sha1) = sha1_file(file) else { return false };
    replaces.get(&w.path).is_some_and(|r| r.eq_ignore_ascii_case(&sha1))
        || w.hash.as_ref().is_some_and(|h| file_matches(file, h))
}

pub async fn apply(
    client: &Client,
    paths: &Paths,
    instance: &mut Instance,
    plan: SyncPlan,
    progress: &ProgressTx,
) -> Result<Vec<Manual>> {
    let game = instance::game_dir(paths, &instance.id);
    let target = |p: &str| safe_path(p).map(|p| game.join(p));
    let deletes = plan.delete.iter().map(|d| target(&d.path)).collect::<Result<Vec<_>>>()?;
    let writes = plan.write.iter().map(|w| target(&w.path)).collect::<Result<Vec<_>>>()?;

    for (d, file) in plan.delete.iter().zip(deletes) {
        let unchanged = file.is_file() && sha1_file(&file).is_ok_and(|s| s.eq_ignore_ascii_case(&d.sha1));
        if !unchanged {
            continue;
        }
        match tokio::fs::remove_file(&file).await {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        }
        let mut dir = file.parent();
        while let Some(d) = dir.filter(|d| *d != game) {
            if tokio::fs::remove_dir(d).await.is_err() {
                break;
            }
            dir = d.parent();
        }
    }

    let old = std::mem::take(&mut instance.managed);
    let mut downloads = Vec::new();
    let mut attempted = vec![false; writes.len()];
    let mut result = Ok(());
    let mut skipped = plan.skipped;
    for (i, (w, file)) in plan.write.iter().zip(&writes).enumerate() {
        if !still_ours(file, w, &plan.replaces) {
            skipped.push(w.path.clone());
            continue;
        }
        attempted[i] = true;
        match &w.from {
            Origin::Bytes(bytes) => {
                if result.is_ok() {
                    result = write_bytes(file, bytes).await;
                }
            }
            Origin::Url(url) => downloads.push(Download {
                url: url.clone(),
                path: file.clone(),
                hash: w.hash.clone(),
                size: w.size, bearer: None,
            }),
            Origin::PanelFile { url, bearer } => downloads.push(Download {
                url: url.clone(),
                path: file.clone(),
                hash: w.hash.clone(),
                size: w.size,
                bearer: Some(bearer.clone()),
            }),
            Origin::Manual { .. } => {}
        }
    }
    if result.is_ok() {
        result = fetch_all(client, "mods", downloads, progress).await;
    }

    let old_sha: HashMap<String, &str> = old.iter().map(|m| (m.path.to_lowercase(), m.sha1.as_str())).collect();
    instance.managed = plan.managed;
    for ((w, file), tried) in plan.write.iter().zip(&writes).zip(attempted) {
        if !file.is_file() {
            continue;
        }
        let arrived = tried && w.hash.as_ref().is_none_or(|h| file_matches(file, h));
        let old_sha = old_sha.get(&w.path.to_lowercase()).copied();
        let Ok(sha1) = sha1_file(file) else { continue };
        if arrived || old_sha.is_some_and(|o| o.eq_ignore_ascii_case(&sha1)) {
            instance.managed.retain(|m| m.path != w.path);
            instance.managed.push(ManagedFile { path: w.path.clone(), sha1 });
        }
    }
    instance.manual = plan.manual.clone();
    instance.skipped = skipped;
    instance::save_synced(paths, instance)?;
    result?;
    Ok(plan.manual)
}

async fn write_bytes(file: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let part = part_path(file);
    tokio::fs::write(&part, bytes).await?;
    tokio::fs::rename(&part, file).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::{Tmp, serve, serve_full, tmp_dir};
    use crate::instance::{Source, create};
    use crate::http_client;
    use sha1::{Digest, Sha1};

    fn sha(bytes: &[u8]) -> String {
        Sha1::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn disk(files: &[(&str, &[u8])]) -> Tmp {
        let t = tmp_dir();
        for (p, b) in files {
            let f = t.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        t
    }

    fn m(path: &str, bytes: &[u8]) -> ManagedFile {
        ManagedFile { path: path.into(), sha1: sha(bytes) }
    }

    fn w(path: &str, bytes: &[u8]) -> Wanted {
        Wanted {
            path: path.into(),
            from: Origin::Url("u".into()),
            hash: Some(Hash::Sha1(sha(bytes))),
            size: None,
        }
    }

    // (write, delete, managed, manual) as plain paths
    fn parts(p: &SyncPlan) -> [Vec<&str>; 4] {
        [
            p.write.iter().map(|w| w.path.as_str()).collect(),
            p.delete.iter().map(|d| d.path.as_str()).collect(),
            p.managed.iter().map(|m| m.path.as_str()).collect(),
            p.manual.iter().map(|m| m.path.as_str()).collect(),
        ]
    }

    fn expect(p: &SyncPlan, write: &[&str], delete: &[&str], managed: &[&str], manual: &[&str]) {
        assert_eq!(parts(p), [write, delete, managed, manual].map(|s| s.to_vec()));
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_path(r".\mods\a.jar").unwrap(), "mods/a.jar");
        for bad in ["", "/etc/x", r"C:\Windows\x", "mods/../../x", "..", r"..\x", "a//b", "a/./b", "a.", "mods/a.jar.", "a /b", "NUL", "nul.txt", "mods/Con.jar", "COM1", "lpt9.x", "aux"] {
            assert!(safe_path(bad).is_err(), "{bad}");
        }
        let err = safe_path("../x").unwrap_err().to_string();
        assert_eq!(err, "unsafe path in pack: ../x");
    }

    fn bytes_w(path: &str, bytes: &[u8]) -> Wanted {
        Wanted { path: path.into(), from: Origin::Bytes(bytes.to_vec()), hash: None, size: None }
    }

    #[test]
    fn an_unchanged_override_with_new_bytes_is_rewritten() {
        let d = disk(&[("a", b"1")]);
        let p = plan(&d, &[m("a", b"1")], vec![bytes_w("a", b"2")]).unwrap();
        expect(&p, &["a"], &[], &[], &[]);
    }

    #[test]
    fn an_override_with_the_same_bytes_is_adopted() {
        let d = disk(&[("a", b"1")]);
        let p = plan(&d, &[m("a", b"1")], vec![bytes_w("a", b"1")]).unwrap();
        expect(&p, &[], &[], &["a"], &[]);
    }

    #[test]
    fn a_changed_override_is_kept() {
        let d = disk(&[("a", b"X")]);
        let p = plan(&d, &[m("a", b"1")], vec![bytes_w("a", b"2")]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn a_big_plan_finishes() {
        let d = disk(&[]);
        let managed: Vec<_> = (0..5000).map(|i| m(&format!("mods/m{i}.jar"), b"1")).collect();
        let wanted = (0..5000).map(|i| w(&format!("mods/N{i}.jar"), b"1")).collect();
        let p = plan(&d, &managed, wanted).unwrap();
        assert_eq!(p.write.len(), 5000);
        assert!(p.delete.is_empty());
    }

    #[test]
    fn a_new_file_is_written() {
        let d = disk(&[]);
        let p = plan(&d, &[], vec![w("a", b"1")]).unwrap();
        expect(&p, &["a"], &[], &[], &[]);
    }

    #[test]
    fn a_right_file_is_adopted() {
        let d = disk(&[("a", b"1")]);
        let p = plan(&d, &[], vec![w("a", b"1")]).unwrap();
        expect(&p, &[], &[], &["a"], &[]);
        assert_eq!(p.managed[0].sha1, sha(b"1"));
    }

    #[test]
    fn a_delete_carries_its_sha1_and_a_replace_records_the_old_one() {
        let d = disk(&[("a", b"1"), ("b", b"1")]);
        let p = plan(&d, &[m("a", b"1"), m("b", b"1")], vec![w("b", b"2")]).unwrap();
        assert_eq!(p.delete, vec![m("a", b"1")]);
        assert_eq!(p.replaces.get("b"), Some(&sha(b"1")));
    }

    #[test]
    fn our_outdated_file_is_replaced() {
        let d = disk(&[("a", b"1")]);
        let p = plan(&d, &[m("a", b"1")], vec![w("a", b"2")]).unwrap();
        expect(&p, &["a"], &[], &[], &[]);
    }

    #[test]
    fn a_changed_managed_file_is_kept() {
        let d = disk(&[("a", b"X")]);
        let p = plan(&d, &[m("a", b"1")], vec![w("a", b"2")]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn the_players_own_file_is_kept() {
        let d = disk(&[("a", b"X")]);
        let p = plan(&d, &[], vec![w("a", b"2")]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn an_unwanted_managed_file_is_deleted() {
        let d = disk(&[("a", b"1")]);
        let p = plan(&d, &[m("a", b"1")], vec![]).unwrap();
        expect(&p, &[], &["a"], &[], &[]);
    }

    #[test]
    fn an_unwanted_changed_file_is_kept() {
        let d = disk(&[("a", b"X")]);
        let p = plan(&d, &[m("a", b"1")], vec![]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn an_unmanaged_file_is_never_deleted() {
        let d = disk(&[("own", b"1")]);
        let p = plan(&d, &[], vec![]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn a_manual_file_is_listed_until_it_is_there() {
        let manual = || Wanted {
            from: Origin::Manual { name: "A".into(), page: "p".into() },
            ..w("a", b"1")
        };
        let p = plan(&disk(&[]), &[], vec![manual()]).unwrap();
        expect(&p, &[], &[], &[], &["a"]);
        assert_eq!((p.manual[0].name.as_str(), p.manual[0].page.as_str()), ("A", "p"));
        let p = plan(&disk(&[("a", b"1")]), &[], vec![manual()]).unwrap();
        expect(&p, &[], &[], &["a"], &[]);
        let nohash = Wanted { hash: None, ..manual() };
        let p = plan(&disk(&[("a", b"1")]), &[], vec![nohash.clone()]).unwrap();
        expect(&p, &[], &[], &[], &[]);
        let p = plan(&disk(&[("a", b"X")]), &[], vec![manual()]).unwrap();
        expect(&p, &[], &[], &[], &[]);
        let p = plan(&disk(&[("a", b"X")]), &[m("a", b"X")], vec![manual()]).unwrap();
        expect(&p, &[], &[], &["a"], &[]);
        let p = plan(&disk(&[("a", b"X")]), &[m("a", b"1")], vec![nohash]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn an_unsafe_path_fails_the_whole_plan() {
        let d = disk(&[("old", b"1")]);
        let r = plan(&d, &[m("old", b"1")], vec![w("good", b"1"), w("../x", b"1")]);
        assert!(r.is_err());
    }

    #[test]
    fn extra_rules() {
        let d = disk(&[("dir/f", b"1"), ("a", b"1"), ("b", b"1")]);
        // a directory at the path is the player's; the later duplicate wins
        let p = plan(&d, &[m("dir", b"1")], vec![w("dir", b"2"), w("a", b"9"), w("a", b"1")]).unwrap();
        expect(&p, &[], &[], &["a"], &[]);
        // no hash, managed and unchanged: left alone and stays managed
        let nohash = Wanted { hash: None, ..w("b", b"1") };
        let p = plan(&d, &[m("b", b"1")], vec![nohash]).unwrap();
        expect(&p, &[], &[], &["b"], &[]);
    }

    #[test]
    fn a_missing_unwanted_managed_file_is_dropped() {
        let p = plan(&disk(&[]), &[m("gone", b"1")], vec![]).unwrap();
        expect(&p, &[], &[], &[], &[]);
    }

    #[test]
    fn paths_compare_without_case() {
        let d = disk(&[("mods/a.jar", b"1")]);
        let upper = |b: &[u8]| w("Mods/A.jar", b);
        let p = plan(&d, &[m("mods/a.jar", b"1")], vec![upper(b"1")]).unwrap();
        expect(&p, &[], &[], &["Mods/A.jar"], &[]);
        let p = plan(&d, &[m("mods/a.jar", b"1")], vec![upper(b"2")]).unwrap();
        expect(&p, &["Mods/A.jar"], &[], &[], &[]);
        let p = plan(&d, &[], vec![w("x", b"1"), w("X", b"1")]).unwrap();
        expect(&p, &["X"], &[], &[], &[]);
    }

    fn setup() -> (Tmp, Paths, Instance) {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        let inst = create(&paths, "T", "1.21.1", None, Source::Manual).unwrap();
        (t, paths, inst)
    }

    fn url_w(path: &str, url: &str, bytes: &[u8]) -> Wanted {
        Wanted { from: Origin::Url(url.into()), ..w(path, bytes) }
    }

    #[tokio::test]
    async fn apply_writes_deletes_and_records() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::create_dir_all(game.join("old")).unwrap();
        std::fs::write(game.join("old/old.jar"), b"old").unwrap();
        std::fs::write(game.join("own.txt"), b"mine").unwrap();
        let (url, _) = serve(200, b"new").await;
        let plan = SyncPlan {
            write: vec![
                url_w("mods/new.jar", &url, b"new"),
                Wanted { from: Origin::Bytes(b"x=1".to_vec()), ..w("config/a.toml", b"x=1") },
            ],
            delete: vec![m("old/old.jar", b"old")],
            managed: vec![],
            manual: vec![Manual { path: "m".into(), name: "M".into(), page: String::new() }],
            replaces: Default::default(),
            skipped: vec![],
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manual = apply(&http_client(), &paths, &mut inst, plan, &tx).await.unwrap();
        assert_eq!(manual.len(), 1);
        assert!(!game.join("old").exists());
        assert!(game.join("own.txt").is_file() && game.join("mods").is_dir());
        assert_eq!(std::fs::read(game.join("mods/new.jar")).unwrap(), b"new");
        assert_eq!(std::fs::read(game.join("config/a.toml")).unwrap(), b"x=1");
        let mut got = instance::load(&paths, &inst.id).unwrap().managed;
        got.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(got, vec![m("config/a.toml", b"x=1"), m("mods/new.jar", b"new")]);
        assert_eq!(inst.managed.len(), 2);
    }

    #[tokio::test]
    async fn a_failed_download_keeps_what_arrived() {
        let (_t, paths, mut inst) = setup();
        let (good, _) = serve(200, b"good").await;
        let (bad, _) = serve(404, b"").await;
        let plan = SyncPlan {
            write: vec![url_w("mods/good.jar", &good, b"good"), url_w("mods/bad.jar", &bad, b"bad")],
            delete: vec![],
            managed: vec![],
            manual: vec![],
            replaces: Default::default(),
            skipped: vec![],
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(apply(&http_client(), &paths, &mut inst, plan, &tx).await.is_err());
        let saved = instance::load(&paths, &inst.id).unwrap().managed;
        assert_eq!(saved, vec![m("mods/good.jar", b"good")]);
    }

    #[tokio::test]
    async fn apply_refuses_an_unsafe_path_before_touching_anything() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a"), b"1").unwrap();
        let plan = SyncPlan {
            write: vec![],
            delete: vec![m("a", b"1"), m("../instance.json", b"1")],
            managed: vec![],
            manual: vec![],
            replaces: Default::default(),
            skipped: vec![],
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(apply(&http_client(), &paths, &mut inst, plan, &tx).await.is_err());
        assert!(game.join("a").is_file());
    }

    #[tokio::test]
    async fn a_failed_replacement_keeps_the_old_file_managed() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a.jar"), b"old").unwrap();
        inst.managed = vec![m("a.jar", b"old")];
        let (bad, _) = serve(404, b"").await;
        let plan = SyncPlan {
            write: vec![url_w("a.jar", &bad, b"new")],
            delete: vec![],
            managed: vec![],
            manual: vec![],
            replaces: [("a.jar".to_string(), sha(b"old"))].into(),
            skipped: vec![],
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(apply(&http_client(), &paths, &mut inst, plan, &tx).await.is_err());
        assert_eq!(std::fs::read(game.join("a.jar")).unwrap(), b"old");
        assert_eq!(instance::load(&paths, &inst.id).unwrap().managed, vec![m("a.jar", b"old")]);
    }

    async fn run(paths: &Paths, inst: &mut Instance, plan: SyncPlan) -> Result<Vec<Manual>> {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        apply(&http_client(), paths, inst, plan, &tx).await
    }

    #[tokio::test]
    async fn a_delete_target_changed_after_plan_is_not_deleted() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a.jar"), b"player edit").unwrap();
        let plan = SyncPlan {
            write: vec![],
            delete: vec![m("a.jar", b"old")],
            managed: vec![],
            manual: vec![],
            replaces: Default::default(),
            skipped: vec![],
        };
        run(&paths, &mut inst, plan).await.unwrap();
        assert_eq!(std::fs::read(game.join("a.jar")).unwrap(), b"player edit");
        assert!(inst.managed.is_empty());
    }

    #[tokio::test]
    async fn a_replace_target_changed_after_plan_is_not_overwritten() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a.jar"), b"player edit").unwrap();
        let wanted = Wanted { from: Origin::Bytes(b"new".to_vec()), ..w("a.jar", b"new") };
        let plan = SyncPlan {
            write: vec![wanted],
            delete: vec![],
            managed: vec![],
            manual: vec![],
            replaces: [("a.jar".to_string(), sha(b"old"))].into(),
            skipped: vec![],
        };
        run(&paths, &mut inst, plan).await.unwrap();
        assert_eq!(std::fs::read(game.join("a.jar")).unwrap(), b"player edit");
        assert!(inst.managed.is_empty());
    }

    #[tokio::test]
    async fn a_file_that_appears_at_a_new_target_is_not_overwritten() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a.jar"), b"player's").unwrap();
        let wanted = Wanted { from: Origin::Bytes(b"new".to_vec()), ..w("a.jar", b"new") };
        let unhashed = Wanted { hash: None, ..wanted.clone() };
        for wanted in [wanted, unhashed] {
            let plan = SyncPlan {
                write: vec![wanted],
                delete: vec![],
                managed: vec![],
                manual: vec![],
                replaces: Default::default(),
                skipped: vec![],
            };
            run(&paths, &mut inst, plan).await.unwrap();
            assert_eq!(std::fs::read(game.join("a.jar")).unwrap(), b"player's");
            assert!(inst.managed.is_empty());
        }
    }

    #[test]
    fn the_players_own_file_is_kept_and_reported() {
        let p = plan(&disk(&[("a", b"X")]), &[], vec![w("a", b"2")]).unwrap();
        expect(&p, &[], &[], &[], &[]);
        assert_eq!(p.skipped, ["a"]);
    }

    #[test]
    fn a_changed_managed_file_is_reported_and_kept() {
        let p = plan(&disk(&[("a", b"X")]), &[m("a", b"1")], vec![w("a", b"2")]).unwrap();
        expect(&p, &[], &[], &[], &[]);
        assert_eq!(p.skipped, ["a"]);
    }

    #[test]
    fn adopted_replaced_new_and_hand_files_are_not_skipped() {
        let d = disk(&[("same", b"1"), ("ours", b"1"), ("hand", b"X")]);
        let manual = Wanted { from: Origin::Manual { name: "H".into(), page: String::new() }, ..w("hand", b"1") };
        let p = plan(&d, &[m("ours", b"1")], vec![w("same", b"1"), w("ours", b"2"), w("new", b"1"), manual]).unwrap();
        assert!(p.skipped.is_empty());
    }

    #[test]
    fn a_folder_in_the_way_is_reported() {
        let p = plan(&disk(&[("dir/f", b"1")]), &[], vec![w("dir", b"2")]).unwrap();
        assert_eq!(p.skipped, ["dir"]);
    }

    #[tokio::test]
    async fn apply_keeps_hand_files_and_skips_in_the_instance() {
        let (_t, paths, mut inst) = setup();
        let plan = SyncPlan {
            manual: vec![Manual { path: "mods/h.jar".into(), name: "H".into(), page: "p".into() }],
            skipped: vec!["mods/own.jar".into()],
            ..Default::default()
        };
        run(&paths, &mut inst, plan).await.unwrap();
        let saved = instance::load(&paths, &inst.id).unwrap();
        assert_eq!(saved.manual.iter().map(|m| m.path.as_str()).collect::<Vec<_>>(), ["mods/h.jar"]);
        assert_eq!(saved.skipped, ["mods/own.jar"]);
    }

    #[tokio::test]
    async fn a_failed_download_still_records_hand_files_and_skips() {
        let (_t, paths, mut inst) = setup();
        let (bad, _) = serve(404, b"").await;
        let plan = SyncPlan {
            write: vec![url_w("mods/bad.jar", &bad, b"bad")],
            manual: vec![Manual { path: "h".into(), name: "H".into(), page: String::new() }],
            skipped: vec!["own".into()],
            ..Default::default()
        };
        assert!(run(&paths, &mut inst, plan).await.is_err());
        let saved = instance::load(&paths, &inst.id).unwrap();
        assert_eq!((saved.manual.len(), saved.skipped), (1, vec!["own".to_string()]));
    }

    #[tokio::test]
    async fn a_file_that_appears_after_the_plan_is_reported() {
        let (_t, paths, mut inst) = setup();
        let game = instance::game_dir(&paths, &inst.id);
        std::fs::write(game.join("a.jar"), b"player's").unwrap();
        let plan = SyncPlan { write: vec![Wanted { from: Origin::Bytes(b"new".to_vec()), ..w("a.jar", b"new") }], ..Default::default() };
        run(&paths, &mut inst, plan).await.unwrap();
        assert_eq!(instance::load(&paths, &inst.id).unwrap().skipped, ["a.jar"]);
    }

    fn panel_plan(url: &str) -> SyncPlan {
        let from = Origin::PanelFile { url: url.into(), bearer: "tok-SECRET".into() };
        SyncPlan {
            write: vec![Wanted { from, ..w("mods/p.jar", b"new") }],
            delete: vec![],
            managed: vec![],
            manual: vec![],
            replaces: Default::default(),
            skipped: vec![],
        }
    }

    #[tokio::test]
    async fn a_panel_file_carries_its_token_and_follows_no_redirect() {
        let (_t, paths, mut inst) = setup();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (url, _, heads) = serve_full(200, b"new".to_vec(), String::new()).await;
        apply(&http_client(), &paths, &mut inst, panel_plan(&url), &tx).await.unwrap();
        assert!(heads.lock().unwrap()[0].to_lowercase().contains("authorization: bearer tok-secret"));
        let game = instance::game_dir(&paths, &inst.id);
        assert_eq!(std::fs::read(game.join("mods/p.jar")).unwrap(), b"new");

        std::fs::remove_file(game.join("mods/p.jar")).unwrap();
        let (other, other_hits) = serve(200, b"new").await;
        let (url, _, _) = serve_full(302, vec![], format!("Location: {other}/x\r\n")).await;
        assert!(apply(&http_client(), &paths, &mut inst, panel_plan(&url), &tx).await.is_err());
        assert_eq!(other_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn debug_hides_the_bearer() {
        let plan = panel_plan("http://x/y");
        assert!(!format!("{plan:?}").contains("tok-SECRET"));
    }
}
