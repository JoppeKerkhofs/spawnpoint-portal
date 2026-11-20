use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::launch::DEFAULT_MEMORY_MB;
use crate::loader::Loader;
use crate::paths::check_relative;
use crate::sync::Manual;
use crate::{Error, Paths, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub source: Source,
    pub managed: Vec<ManagedFile>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub last_played: Option<LastPlayed>,
    #[serde(default)]
    pub manual: Vec<Manual>,
    #[serde(default)]
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastPlayed {
    pub started_at: u64,
    pub seconds: u64,
}

pub const MEMORY_CHOICES_MB: [u32; 4] = [4096, 6144, 8192, 10240];

impl Instance {
    pub fn memory_mb(&self) -> u32 {
        self.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    Manual,
    Pack { pack: PackRef },
    Server { panel: String, server: String, pack: Option<PackRef> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackRef {
    pub catalogue: String,
    pub project: String,
    pub version: String,
    #[serde(default)]
    pub version_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedFile {
    pub path: String,
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModFile {
    pub name: String,
    pub size: u64,
    pub managed: bool,
}

pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().map(|c| c.to_ascii_lowercase()) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() { "instance".into() } else { out.into() }
}

fn dir(paths: &Paths, id: &str) -> PathBuf {
    paths.instances().join(id)
}

pub fn game_dir(paths: &Paths, id: &str) -> PathBuf {
    dir(paths, id).join("game")
}

pub fn create(
    paths: &Paths,
    name: &str,
    minecraft: &str,
    loader: Option<Loader>,
    source: Source,
) -> Result<Instance> {
    std::fs::create_dir_all(paths.instances())?;
    let base = slug(name);
    for n in 1.. {
        let id = if n == 1 { base.clone() } else { format!("{base}-{n}") };
        match std::fs::create_dir(dir(paths, &id)) {
            Ok(()) => {
                std::fs::create_dir_all(game_dir(paths, &id).join("mods"))?;
                let instance = Instance {
                    id,
                    name: name.into(),
                    minecraft: minecraft.into(),
                    loader,
                    source,
                    managed: Vec::new(),
                    memory_mb: None,
                    last_played: None,
                    manual: Vec::new(),
                    skipped: Vec::new(),
                };
                save(paths, &instance)?;
                return Ok(instance);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

pub fn load(paths: &Paths, id: &str) -> Result<Instance> {
    let missing = || Error::Other(format!("no instance {id}"));
    if id != slug(id) {
        return Err(missing());
    }
    match std::fs::read(dir(paths, id).join("instance.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(missing()),
        Err(e) => Err(e.into()),
    }
}

pub fn save(paths: &Paths, instance: &Instance) -> Result<()> {
    if instance.id != slug(&instance.id) {
        return Err(Error::Other(format!("{:?} is not an instance id", instance.id)));
    }
    let folder = dir(paths, &instance.id);
    let tmp = folder.join("instance.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(instance)?)?;
    std::fs::rename(tmp, folder.join("instance.json"))?;
    Ok(())
}

// ponytail: no lock between two update calls in one process; add one if the window ever writes from two places at once.
pub fn update(paths: &Paths, id: &str, change: impl FnOnce(&mut Instance)) -> Result<Instance> {
    let mut instance = load(paths, id)?;
    change(&mut instance);
    save(paths, &instance)?;
    Ok(instance)
}

pub fn set_memory(paths: &Paths, id: &str, mb: u32) -> Result<Instance> {
    if !MEMORY_CHOICES_MB.contains(&mb) {
        return Err(Error::Other(format!("{mb} MB is not one of the memory choices")));
    }
    update(paths, id, |i| i.memory_mb = Some(mb))
}

pub fn record_played(paths: &Paths, id: &str, started_at: u64, seconds: u64) -> Result<Instance> {
    update(paths, id, |i| i.last_played = Some(LastPlayed { started_at, seconds }))
}

pub fn check_manual(paths: &Paths, id: &str) -> Result<Instance> {
    let game = game_dir(paths, id);
    update(paths, id, |i| i.manual.retain(|m| !game.join(&m.path).exists()))
}

static SYNCING: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub struct InstanceLock(String);

impl Drop for InstanceLock {
    fn drop(&mut self) {
        SYNCING.lock().unwrap().retain(|i| *i != self.0);
    }
}

pub fn lock(id: &str) -> Result<InstanceLock> {
    let mut held = SYNCING.lock().unwrap();
    if held.iter().any(|i| i == id) {
        return Err(Error::Busy(id.into()));
    }
    held.push(id.into());
    Ok(InstanceLock(id.into()))
}

// A sync works on an instance loaded earlier: what the player set meanwhile stays.
pub fn save_synced(paths: &Paths, instance: &Instance) -> Result<()> {
    let mut merged = instance.clone();
    if let Ok(disk) = load(paths, &instance.id) {
        merged.memory_mb = disk.memory_mb;
        merged.last_played = disk.last_played;
    }
    save(paths, &merged)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    pub instances: Vec<Instance>,
    pub unreadable: Vec<Unreadable>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unreadable {
    pub folder: String,
}

pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
    list_all(paths).map(|l| l.instances)
}

pub fn list_all(paths: &Paths) -> Result<Listing> {
    let entries = match std::fs::read_dir(paths.instances()) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Listing { instances: Vec::new(), unreadable: Vec::new() }),
        Err(e) => return Err(e.into()),
    };
    let (mut instances, mut unreadable) = (Vec::new(), Vec::new());
    for entry in entries {
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().map(String::from) else { continue };
        if !entry.file_type()?.is_dir() {
            continue;
        }
        match load(paths, &id).ok().filter(|i| i.id == id) {
            Some(i) => instances.push(i),
            None => unreadable.push(Unreadable { folder: id }),
        }
    }
    instances.sort_by(|a, b| a.name.cmp(&b.name));
    unreadable.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(Listing { instances, unreadable })
}

pub fn folder(paths: &Paths, id: &str) -> Result<PathBuf> {
    check_relative(id)?;
    if id.contains('/') {
        return Err(Error::Other(format!("{id:?} is not a plain folder name")));
    }
    let path = dir(paths, id);
    if !path.is_dir() {
        return Err(Error::Other(format!("no instance folder {id}")));
    }
    Ok(path)
}

pub fn delete(paths: &Paths, id: &str) -> Result<()> {
    if id != slug(id) {
        return Err(Error::Other(format!("{id:?} is not an instance id")));
    }
    std::fs::remove_dir_all(dir(paths, id))?;
    Ok(())
}

fn has_ext(name: &str, ext: &str) -> bool {
    name.to_ascii_lowercase().ends_with(ext)
}

pub fn mods(paths: &Paths, instance: &Instance) -> Result<Vec<ModFile>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(game_dir(paths, &instance.id).join("mods")) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let Some(name) = entry.file_name().to_str().map(String::from) else { continue };
        if meta.is_file() && (has_ext(&name, ".jar") || has_ext(&name, ".jar.disabled")) {
            let rel = format!("mods/{name}").to_lowercase();
            let managed = instance.managed.iter().any(|m| m.path.to_lowercase() == rel);
            out.push(ModFile { name, size: meta.len(), managed });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn add_mod(paths: &Paths, instance: &Instance, file: &Path) -> Result<()> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| has_ext(n, ".jar"))
        .ok_or_else(|| Error::Other(format!("{} is not a .jar file", file.display())))?;
    let mods = game_dir(paths, &instance.id).join("mods");
    std::fs::create_dir_all(&mods)?;
    let target = mods.join(name);
    if target.exists() {
        return Err(Error::Other(format!("{name} is already in this instance")));
    }
    std::fs::copy(file, target)?;
    Ok(())
}

pub fn remove_mod(paths: &Paths, instance: &mut Instance, name: &str) -> Result<()> {
    check_relative(name)?;
    if name.contains('/') {
        return Err(Error::Other(format!("{name:?} is not a plain file name")));
    }
    match std::fs::remove_file(game_dir(paths, &instance.id).join("mods").join(name)) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(Error::Other(format!("{name} is not in this instance")));
        }
        Err(e) => return Err(e.into()),
    }
    let rel = format!("mods/{name}");
    instance.managed.retain(|m| m.path != rel);
    save(paths, instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp() -> (Tmp, Paths) {
        let dir = std::env::temp_dir().join(format!(
            "portal-inst-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths { root: dir.join("root") };
        (Tmp(dir), paths)
    }

    fn jar_named(t: &Tmp, name: &str) -> PathBuf {
        let p = t.0.join(name);
        std::fs::write(&p, b"jar").unwrap();
        p
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("All the Mods 10!"), "all-the-mods-10");
        assert_eq!(slug("  ../..  "), "instance");
    }

    #[test]
    fn create_load_list_delete() {
        let (_t, p) = tmp();
        assert!(list(&p).unwrap().is_empty());
        let a = create(&p, "My Pack", "1.21.1", Some(Loader::parse("neoforge:21.1.90").unwrap()), Source::Manual).unwrap();
        let b = create(&p, "My Pack", "1.21.1", None, Source::Manual).unwrap();
        assert_eq!((a.id.as_str(), b.id.as_str()), ("my-pack", "my-pack-2"));
        assert!(game_dir(&p, "my-pack").join("mods").is_dir());
        assert_eq!(load(&p, "my-pack").unwrap().loader.unwrap().version.as_deref(), Some("21.1.90"));
        assert_eq!(list(&p).unwrap().len(), 2);
        delete(&p, "my-pack").unwrap();
        assert!(load(&p, "my-pack").is_err());
        assert!(load(&p, "../my-pack-2").is_err());
        assert!(delete(&p, "..").is_err());
        assert!(delete(&p, "My Pack").is_err());
    }

    #[test]
    fn a_tampered_id_is_refused() {
        let (_t, p) = tmp();
        let mut a = create(&p, "A", "1.21.1", None, Source::Manual).unwrap();
        a.id = "../x".into();
        assert!(save(&p, &a).is_err());
        assert!(!p.instances().join("../x").exists());
        let mut other = create(&p, "B", "1.21.1", None, Source::Manual).unwrap();
        other.id = "a".into();
        std::fs::write(p.instances().join("b").join("instance.json"), serde_json::to_vec(&other).unwrap()).unwrap();
        assert_eq!(list(&p).unwrap().iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn a_missing_mods_folder_is_an_empty_list() {
        let (_t, p) = tmp();
        let inst = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        std::fs::remove_dir_all(game_dir(&p, "m").join("mods")).unwrap();
        assert!(mods(&p, &inst).unwrap().is_empty());
    }

    #[test]
    fn source_round_trips() {
        let (_t, p) = tmp();
        let pack = PackRef { catalogue: "ftb".into(), project: "1".into(), version: "2".into(), version_name: None };
        let source = Source::Server { panel: "p1".into(), server: "atm10".into(), pack: Some(pack) };
        let mut i = create(&p, "S", "1.21.1", None, source.clone()).unwrap();
        i.managed.push(ManagedFile { path: "mods/a.jar".into(), sha1: "ab".into() });
        save(&p, &i).unwrap();
        assert_eq!(load(&p, "s").unwrap(), i);
        assert_eq!(load(&p, "s").unwrap().source, source);
        assert!(!game_dir(&p, "s").parent().unwrap().join("instance.json.tmp").exists());
    }

    #[test]
    fn own_mods() {
        let (t, p) = tmp();
        let mut inst = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        add_mod(&p, &inst, &jar_named(&t, "own.jar")).unwrap();
        assert!(add_mod(&p, &inst, &jar_named(&t, "own.jar")).is_err());
        assert!(add_mod(&p, &inst, &jar_named(&t, "notes.txt")).is_err());
        std::fs::write(game_dir(&p, "m").join("mods").join("off.jar.disabled"), b"x").unwrap();
        inst.managed.push(ManagedFile { path: "mods/off.jar.disabled".into(), sha1: "x".into() });
        let m = mods(&p, &inst).unwrap();
        assert!(m.iter().any(|f| f.name == "own.jar" && !f.managed && f.size == 3));
        assert!(m.iter().any(|f| f.name == "off.jar.disabled" && f.managed));
        inst.managed.push(ManagedFile { path: "Mods/OWN.JAR".into(), sha1: "x".into() });
        assert!(mods(&p, &inst).unwrap().iter().any(|f| f.name == "own.jar" && f.managed));
        inst.managed.pop();
        assert!(remove_mod(&p, &mut inst, r"..\instance.json").is_err());
        assert!(remove_mod(&p, &mut inst, "../instance.json").is_err());
        assert!(remove_mod(&p, &mut inst, "gone.jar").is_err());
        remove_mod(&p, &mut inst, "own.jar").unwrap();
        remove_mod(&p, &mut inst, "off.jar.disabled").unwrap();
        assert!(mods(&p, &inst).unwrap().is_empty());
        assert!(inst.managed.is_empty());
        assert!(load(&p, "m").unwrap().managed.is_empty());
    }

    #[test]
    fn memory_is_one_of_the_choices() {
        let (_t, p) = tmp();
        let i = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        assert_eq!(i.memory_mb(), 4096);
        assert_eq!(set_memory(&p, "m", 6144).unwrap().memory_mb(), 6144);
        assert!(set_memory(&p, "m", 5000).is_err());
        assert_eq!(load(&p, "m").unwrap().memory_mb, Some(6144));
    }

    #[test]
    fn last_play_is_recorded() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        record_played(&p, "m", 1_763_640_000, 4320).unwrap();
        assert_eq!(load(&p, "m").unwrap().last_played, Some(LastPlayed { started_at: 1_763_640_000, seconds: 4320 }));
    }

    #[test]
    fn a_sync_keeps_memory_and_play_set_meanwhile() {
        let (_t, p) = tmp();
        let mut stale = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        set_memory(&p, "m", 8192).unwrap();
        record_played(&p, "m", 10, 20).unwrap();
        stale.minecraft = "1.20.1".into();
        save_synced(&p, &stale).unwrap();
        let now = load(&p, "m").unwrap();
        assert_eq!((now.minecraft.as_str(), now.memory_mb, now.last_played.map(|l| l.seconds)), ("1.20.1", Some(8192), Some(20)));
    }

    #[test]
    fn old_instance_files_still_load() {
        let (_t, p) = tmp();
        std::fs::create_dir_all(p.instances().join("old")).unwrap();
        let old = r#"{"id":"old","name":"Old","minecraft":"1.21.1","loader":null,
            "source":{"kind":"server","panel":"p","server":"s","pack":{"catalogue":"ftb","project":"1","version":"2"}},"managed":[]}"#;
        std::fs::write(p.instances().join("old/instance.json"), old).unwrap();
        let i = load(&p, "old").unwrap();
        assert_eq!((i.memory_mb, i.last_played), (None, None));
        assert!(i.manual.is_empty() && i.skipped.is_empty());
        assert!(matches!(i.source, Source::Server { pack: Some(PackRef { version_name: None, .. }), .. }));
    }

    #[test]
    fn unreadable_folders_are_listed_beside_the_instances() {
        let (_t, p) = tmp();
        create(&p, "Good", "1.21.1", None, Source::Manual).unwrap();
        std::fs::create_dir_all(p.instances().join("old-test")).unwrap();
        std::fs::write(p.instances().join("old-test/instance.json"), b"{not json").unwrap();
        std::fs::create_dir_all(p.instances().join("empty")).unwrap();
        std::fs::write(p.instances().join("stray.txt"), b"x").unwrap();
        let l = list_all(&p).unwrap();
        assert_eq!(l.instances.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["good"]);
        assert_eq!(l.unreadable, vec![Unreadable { folder: "empty".into() }, Unreadable { folder: "old-test".into() }]);
    }

    #[test]
    fn check_manual_drops_files_that_arrived() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        let hand = |path: &str| Manual { path: path.into(), name: "x".into(), page: String::new() };
        update(&p, "m", |i| i.manual = vec![hand("mods/a.jar"), hand("mods/b.jar")]).unwrap();
        std::fs::write(game_dir(&p, "m").join("mods/a.jar"), b"x").unwrap();
        let i = check_manual(&p, "m").unwrap();
        assert_eq!(i.manual, vec![hand("mods/b.jar")]);
        assert_eq!(load(&p, "m").unwrap().manual, vec![hand("mods/b.jar")]);
    }

    #[test]
    fn one_sync_per_instance() {
        let first = lock("lock-test").unwrap();
        assert!(matches!(lock("lock-test"), Err(Error::Busy(_))));
        assert!(lock("lock-test-2").is_ok());
        drop(first);
        assert!(lock("lock-test").is_ok());
    }

    #[test]
    fn folders_are_plain_names_that_exist() {
        let (_t, p) = tmp();
        std::fs::create_dir_all(p.instances().join("old-test")).unwrap();
        std::fs::create_dir_all(p.instances().join("a/b")).unwrap();
        assert!(folder(&p, "old-test").unwrap().ends_with("old-test"));
        for bad in ["", "..", r"..\x", "a/b", "missing"] {
            assert!(folder(&p, bad).is_err(), "{bad}");
        }
    }
}
