use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::launch::{DEFAULT_MEMORY_MB, server_arg_allowed, split_args};
use crate::loader::{Loader, LoaderKind};
use crate::paths::{check_id, check_relative};
use crate::sync::{Manual, path_key, safe_path};
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
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub own_picture: bool,
    // The player's own Java arguments, as typed.
    #[serde(default)]
    pub java_args: String,
    // What the server sent at the last sync: the arguments in use, and those Portal refused.
    #[serde(default)]
    pub server_java_args: Vec<String>,
    #[serde(default)]
    pub refused_java_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastPlayed {
    pub started_at: u64,
    pub seconds: u64,
}

// What the browser installs into an instance: each kind has its own folder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Resourcepack,
    Shader,
}

pub const MEMORY_CHOICES_MB: [u32; 4] = [4096, 6144, 8192, 10240];

impl Instance {
    pub fn memory_mb(&self) -> u32 {
        self.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)
    }

    // The server's first, so the player's own choice wins where both name a flag. Checked again: instance.json is a file.
    pub fn java_arguments(&self) -> Vec<String> {
        let server = self.server_java_args.iter().filter(|a| server_arg_allowed(a)).cloned();
        server.chain(split_args(&self.java_args)).collect()
    }

    pub fn set_server_java_args(&mut self, sent: Option<&str>) {
        (self.server_java_args, self.refused_java_args) = split_args(sent.unwrap_or_default()).into_iter().partition(|a| server_arg_allowed(a));
    }

    pub fn is_horizon(&self) -> bool {
        matches!(self.loader, Some(Loader { kind: LoaderKind::Horizon, .. }))
    }

    // The folder under the game folder that holds the mods, and how a mod file ends.
    pub fn mods_place(&self) -> (&'static str, &'static str) {
        if self.is_horizon() { ("horizon-mods", ".hmod") } else { ("mods", ".jar") }
    }

    pub fn place(&self, kind: Kind) -> Result<(&'static str, &'static str)> {
        match kind {
            Kind::Mod => Ok(self.mods_place()),
            _ if self.is_horizon() => Err(Error::Unsupported("Horizon has no resource packs or shaders".into())),
            Kind::Resourcepack => Ok(("resourcepacks", ".zip")),
            Kind::Shader => Ok(("shaderpacks", ".zip")),
        }
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
                    picture: None,
                    own_picture: false,
                    java_args: String::new(),
                    server_java_args: Vec::new(),
                    refused_java_args: Vec::new(),
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

pub fn create_manual(paths: &Paths, name: &str, minecraft: &str, loader: Option<Loader>) -> Result<Instance> {
    let horizon = matches!(loader, Some(Loader { kind: LoaderKind::Horizon, .. }));
    let minecraft = if horizon { crate::horizon::MINECRAFT } else { minecraft };
    check_id(minecraft)?;
    create(paths, name, minecraft, loader, Source::Manual)
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

// One load-then-save at a time in this process; never held across an await.
static SAVING: Mutex<()> = Mutex::new(());

fn saving() -> std::sync::MutexGuard<'static, ()> {
    SAVING.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn update(paths: &Paths, id: &str, change: impl FnOnce(&mut Instance)) -> Result<Instance> {
    let _saving = saving();
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

pub fn set_java_args(paths: &Paths, id: &str, args: &str) -> Result<Instance> {
    if args.len() > 2000 || args.chars().any(char::is_control) {
        return Err(Error::Other("Java arguments are one line of at most 2000 characters".into()));
    }
    update(paths, id, |i| i.java_args = args.trim().into())
}

pub fn set_picture(paths: &Paths, id: &str, picture: Option<String>) -> Result<Instance> {
    if let Some(p) = &picture
        && !(p.starts_with("data:") && crate::panel::picture(p).is_some())
    {
        return Err(Error::Other("that is not a picture the launcher can show".into()));
    }
    update(paths, id, |i| (i.own_picture, i.picture) = (picture.is_some(), picture))
}

pub fn rename(paths: &Paths, id: &str, name: &str) -> Result<Instance> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("an instance needs a name".into()));
    }
    update(paths, id, |i| i.name = name.into())
}

// Only for an instance the player made: a sync decides the version of a server's or a pack's.
pub fn set_version(paths: &Paths, id: &str, minecraft: &str, loader: Option<Loader>) -> Result<Instance> {
    check_id(minecraft)?;
    let horizon = matches!(loader, Some(Loader { kind: LoaderKind::Horizon, .. }));
    let current = load(paths, id)?;
    if current.source != Source::Manual || current.is_horizon() || horizon {
        return Err(Error::Other("only an instance you made yourself can change its version".into()));
    }
    update(paths, id, |i| (i.minecraft, i.loader) = (minecraft.into(), loader))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// A copy of a server's instance belongs to the player: a server has one instance, and a sync finds it by its source.
pub fn duplicate(paths: &Paths, id: &str, name: &str) -> Result<Instance> {
    let from = load(paths, id)?;
    let _lock = lock(id)?;
    let own = matches!(from.source, Source::Server { .. });
    let source = if own { Source::Manual } else { from.source.clone() };
    let mut copy = create(paths, name.trim(), &from.minecraft, from.loader.clone(), source)?;
    if let Err(e) = copy_dir(&game_dir(paths, id), &game_dir(paths, &copy.id)) {
        let _ = std::fs::remove_dir_all(dir(paths, &copy.id));
        return Err(e.into());
    }
    copy.managed = if own { Vec::new() } else { from.managed };
    (copy.memory_mb, copy.manual, copy.skipped, copy.picture, copy.own_picture) = (from.memory_mb, from.manual, from.skipped, from.picture, from.own_picture);
    copy.java_args = from.java_args;
    save(paths, &copy)?;
    Ok(copy)
}

pub fn record_played(paths: &Paths, id: &str, started_at: u64, seconds: u64) -> Result<Instance> {
    update(paths, id, |i| i.last_played = Some(LastPlayed { started_at, seconds }))
}

pub fn check_manual(paths: &Paths, id: &str) -> Result<Instance> {
    let game = game_dir(paths, id);
    update(paths, id, |i| i.manual.retain(|m| safe_path(&m.path).is_ok_and(|p| !game.join(p).exists())))
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
pub fn save_synced(paths: &Paths, instance: &mut Instance) -> Result<()> {
    let _saving = saving();
    if let Ok(disk) = load(paths, &instance.id) {
        instance.memory_mb = disk.memory_mb;
        instance.java_args = disk.java_args;
        instance.last_played = disk.last_played;
        if disk.own_picture || instance.own_picture {
            instance.picture = disk.picture;
        }
        instance.own_picture = disk.own_picture;
    }
    save(paths, instance)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listing {
    pub instances: Vec<Instance>,
    pub unreadable: Vec<Unreadable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    let _lock = lock(id)?;
    std::fs::remove_dir_all(dir(paths, id))?;
    Ok(())
}

fn has_ext(name: &str, ext: &str) -> bool {
    name.to_ascii_lowercase().ends_with(ext)
}

pub fn mods(paths: &Paths, instance: &Instance) -> Result<Vec<ModFile>> {
    files(paths, instance, Kind::Mod)
}

pub fn files(paths: &Paths, instance: &Instance, kind: Kind) -> Result<Vec<ModFile>> {
    let mut out = Vec::new();
    let (folder, ext) = instance.place(kind)?;
    let entries = match std::fs::read_dir(game_dir(paths, &instance.id).join(folder)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let Some(name) = entry.file_name().to_str().map(String::from) else { continue };
        if meta.is_file() && (has_ext(&name, ext) || has_ext(&name, &format!("{ext}.disabled"))) {
            let rel = path_key(&format!("{folder}/{name}"));
            let managed = instance.managed.iter().any(|m| path_key(&m.path) == rel);
            out.push(ModFile { name, size: meta.len(), managed });
        }
    }
    out.sort_by_cached_key(|f| f.name.to_lowercase());
    Ok(out)
}

pub fn add_mod(paths: &Paths, instance: &Instance, file: &Path) -> Result<()> {
    add_file(paths, instance, Kind::Mod, file)
}

pub fn add_file(paths: &Paths, instance: &Instance, kind: Kind, file: &Path) -> Result<()> {
    let (folder, ext) = instance.place(kind)?;
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| has_ext(n, ext))
        .ok_or_else(|| Error::Other(format!("{} is not a {ext} file", file.display())))?;
    let game = game_dir(paths, &instance.id);
    let mods = game.join(folder);
    std::fs::create_dir_all(&mods)?;
    let target = mods.join(name);
    if target.exists() {
        return Err(Error::Other(format!("{name} is already in this instance")));
    }
    std::fs::copy(file, target)?;
    if instance.is_horizon() {
        // Only a mod gets here. The file of a Horizon mod is named after its id.
        crate::horizon::enable_mod(&game, &name[..name.len() - ext.len()])?;
    }
    Ok(())
}

pub fn remove_mod(paths: &Paths, instance: &mut Instance, name: &str) -> Result<()> {
    remove_file(paths, instance, Kind::Mod, name)
}

pub fn remove_file(paths: &Paths, instance: &mut Instance, kind: Kind, name: &str) -> Result<()> {
    check_relative(name)?;
    if name.contains('/') {
        return Err(Error::Other(format!("{name:?} is not a plain file name")));
    }
    let folder = instance.place(kind)?.0;
    match std::fs::remove_file(game_dir(paths, &instance.id).join(folder).join(name)) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(Error::Other(format!("{name} is not in this instance")));
        }
        Err(e) => return Err(e.into()),
    }
    let rel = format!("{folder}/{name}");
    instance.managed.retain(|m| m.path != rel);
    update(paths, &instance.id, |i| i.managed.retain(|m| m.path != rel)).map(|_| ())
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

    #[test]
    fn rename_duplicate_and_change_version() {
        let (_t, paths) = tmp();
        let server = Source::Server { panel: "p".into(), server: "s".into(), pack: None };
        let mut a = create(&paths, "Dup A", "1.21.1", None, server).unwrap();
        a.managed.push(ManagedFile { path: "mods/a.jar".into(), sha1: "x".into() });
        save(&paths, &a).unwrap();
        std::fs::create_dir_all(game_dir(&paths, &a.id).join("saves/w")).unwrap();
        std::fs::write(game_dir(&paths, &a.id).join("saves/w/level.dat"), b"world").unwrap();

        assert!(rename(&paths, &a.id, "  ").is_err());
        assert_eq!(rename(&paths, &a.id, " New ").unwrap().name, "New");
        assert!(load(&paths, "dup-a").is_ok());

        let copy = duplicate(&paths, &a.id, "Dup B").unwrap();
        assert_eq!((copy.id.as_str(), &copy.source, copy.managed.len()), ("dup-b", &Source::Manual, 0));
        assert_eq!(std::fs::read(game_dir(&paths, "dup-b").join("saves/w/level.dat")).unwrap(), b"world");
        assert_eq!(load(&paths, "dup-b").unwrap(), copy);

        assert!(set_version(&paths, &a.id, "1.20.1", None).is_err());

        // A sync replaces the server's arguments and keeps the player's, which come last at the start.
        assert!(set_java_args(&paths, &a.id, "-Dx\n-Dy").is_err());
        set_java_args(&paths, &a.id, " -Dmine=1 ").unwrap();
        let mut synced = a.clone();
        synced.set_server_java_args(Some("-XX:+UseZGC -Xmx64G -javaagent:evil.jar"));
        save_synced(&paths, &mut synced).unwrap();
        let now = load(&paths, &a.id).unwrap();
        assert_eq!(now.java_arguments(), ["-XX:+UseZGC", "-Dmine=1"]);
        assert_eq!(now.refused_java_args, ["-Xmx64G", "-javaagent:evil.jar"]);
        synced.set_server_java_args(None);
        assert!(synced.server_java_args.is_empty() && synced.refused_java_args.is_empty());
        assert!(set_version(&paths, "dup-b", "../x", None).is_err());
        let fabric = Loader::parse("fabric:0.1").unwrap();
        let changed = set_version(&paths, "dup-b", "1.20.1", Some(fabric.clone())).unwrap();
        assert_eq!((changed.minecraft.as_str(), changed.loader), ("1.20.1", Some(fabric)));
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
    fn a_manual_instance_needs_a_valid_version() {
        let (_t, p) = tmp();
        for bad in ["", "..", "1.21 1", "1/21"] {
            assert!(create_manual(&p, "A", bad, None).is_err(), "{bad:?}");
        }
        assert!(list(&p).unwrap().is_empty());
        assert_eq!(create_manual(&p, "A", "1.21.1", None).unwrap().source, Source::Manual);
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
    fn mods_sort_case_insensitively() {
        let (t, p) = tmp();
        let inst = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        add_mod(&p, &inst, &jar_named(&t, "FarmersDelight.jar")).unwrap();
        add_mod(&p, &inst, &jar_named(&t, "appleskin.jar")).unwrap();
        let names: Vec<_> = mods(&p, &inst).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["appleskin.jar", "FarmersDelight.jar"]);
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
        // One file where the disk ignores case, two on Linux.
        assert_eq!(mods(&p, &inst).unwrap().iter().any(|f| f.name == "own.jar" && f.managed), cfg!(not(target_os = "linux")));
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
    fn a_horizon_instance_keeps_hmod_files_in_its_own_folder_and_turns_them_on() {
        let (t, p) = tmp();
        let mut inst = create_manual(&p, "H", "", Some(Loader::parse("horizon").unwrap())).unwrap();
        assert_eq!((inst.minecraft.as_str(), inst.is_horizon()), (crate::horizon::MINECRAFT, true));
        assert!(add_mod(&p, &inst, &jar_named(&t, "own.jar")).is_err());
        add_mod(&p, &inst, &jar_named(&t, "lights.hmod")).unwrap();
        let game = game_dir(&p, "h");
        assert_eq!(std::fs::read_to_string(game.join("horizon-mods.txt")).unwrap(), "mod.lights:true\n");
        assert_eq!(mods(&p, &inst).unwrap().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["lights.hmod"]);
        remove_mod(&p, &mut inst, "lights.hmod").unwrap();
        assert!(!game.join("horizon-mods/lights.hmod").exists());
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
        save_synced(&p, &mut stale).unwrap();
        assert_eq!((stale.memory_mb, stale.last_played.as_ref().map(|l| l.seconds)), (Some(8192), Some(20)));
        let now = load(&p, "m").unwrap();
        assert_eq!((now.minecraft.as_str(), now.memory_mb, now.last_played.map(|l| l.seconds)), ("1.20.1", Some(8192), Some(20)));
    }

    #[test]
    fn a_sync_replaces_a_server_picture_but_not_an_own_one() {
        let (_t, p) = tmp();
        let mut i = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        i.picture = Some("https://cdn.modrinth.com/old.png".into());
        save(&p, &i).unwrap();
        let mut synced = load(&p, "m").unwrap();
        crate::join::take_picture(&mut synced, Some("https://cdn.modrinth.com/new.png".into()));
        save_synced(&p, &mut synced).unwrap();
        assert_eq!(load(&p, "m").unwrap().picture.as_deref(), Some("https://cdn.modrinth.com/new.png"));
        crate::join::take_picture(&mut synced, None);
        assert_eq!(synced.picture, None);

        let mut stale = load(&p, "m").unwrap();
        set_picture(&p, "m", Some("data:image/png;base64,AAAA".into())).unwrap();
        crate::join::take_picture(&mut stale, Some("https://cdn.modrinth.com/x.png".into()));
        save_synced(&p, &mut stale).unwrap();
        let own = load(&p, "m").unwrap();
        assert_eq!((own.picture.as_deref(), own.own_picture), (Some("data:image/png;base64,AAAA"), true));
        assert_eq!((stale.picture.as_deref(), stale.own_picture), (Some("data:image/png;base64,AAAA"), true));
        crate::join::take_picture(&mut stale, None);
        assert_eq!(stale.picture.as_deref(), Some("data:image/png;base64,AAAA"));

        assert!(set_picture(&p, "m", Some("https://cdn.modrinth.com/a.png".into())).is_err());
        let back = set_picture(&p, "m", None).unwrap();
        assert_eq!((back.picture, back.own_picture), (None, false));
    }

    #[test]
    fn a_picture_removed_during_a_sync_stays_removed() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        set_picture(&p, "m", Some("data:image/png;base64,AAAA".into())).unwrap();
        let mut synced = load(&p, "m").unwrap();
        set_picture(&p, "m", None).unwrap();
        save_synced(&p, &mut synced).unwrap();
        let back = load(&p, "m").unwrap();
        assert_eq!((back.picture, back.own_picture), (None, false));
    }

    #[test]
    fn an_instance_file_without_pictures_loads() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        let file = dir(&p, "m").join("instance.json");
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        v.as_object_mut().unwrap().retain(|k, _| k != "picture" && k != "own_picture");
        std::fs::write(&file, serde_json::to_vec(&v).unwrap()).unwrap();
        let i = load(&p, "m").unwrap();
        assert_eq!((i.picture, i.own_picture), (None, false));
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
    fn saves_of_one_instance_do_not_interleave() {
        use std::sync::atomic::AtomicBool;
        let (_t, p) = tmp();
        let mut synced = create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        let managed = |i: usize| vec![ManagedFile { path: format!("m{i}"), sha1: "x".into() }];
        let index = |i: &Instance| i.managed.first().map_or(0, |m| m.path[1..].parse::<usize>().unwrap());
        let done = AtomicBool::new(false);
        struct Stop<'a>(&'a AtomicBool);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        std::thread::scope(|s| {
            let _stop = Stop(&done);
            let observer = s.spawn(|| {
                let (mut played, mut synced_at) = (0, 0);
                while !done.load(Ordering::SeqCst) {
                    if let Ok(i) = load(&p, "m") {
                        let now = i.last_played.as_ref().map_or(0, |l| l.started_at);
                        assert!(now >= played, "a last play was lost");
                        assert!(index(&i) >= synced_at, "a sync's managed list was reverted");
                        (played, synced_at) = (now, index(&i));
                    }
                }
            });
            let writers = [
                s.spawn(|| (1..=300).for_each(|n| { set_memory(&p, "m", MEMORY_CHOICES_MB[n % 4]).unwrap(); })),
                s.spawn(|| (1..=300).for_each(|n| { record_played(&p, "m", n, 0).unwrap(); })),
                s.spawn(|| (1..=300).for_each(|_| { check_manual(&p, "m").unwrap(); })),
                s.spawn(|| (1..=300).for_each(|_| { check_manual(&p, "m").unwrap(); })),
            ];
            for i in 1..=300 {
                synced.managed = managed(i);
                save_synced(&p, &mut synced).unwrap();
            }
            writers.into_iter().for_each(|w| w.join().unwrap());
            drop(_stop);
            observer.join().unwrap();
        });
        let end = load(&p, "m").unwrap();
        assert_eq!((end.memory_mb, end.last_played.map(|l| l.started_at), end.managed), (Some(MEMORY_CHOICES_MB[0]), Some(300), managed(300)));
    }

    #[test]
    fn a_locked_instance_is_not_deleted() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        let held = lock("m").unwrap();
        assert!(matches!(delete(&p, "m"), Err(Error::Busy(_))));
        assert!(load(&p, "m").is_ok());
        drop(held);
        delete(&p, "m").unwrap();
        assert!(load(&p, "m").is_err());
    }

    #[test]
    fn check_manual_drops_unsafe_paths() {
        let (_t, p) = tmp();
        create(&p, "M", "1.21.1", None, Source::Manual).unwrap();
        let hand = |path: &str| Manual { path: path.into(), name: "x".into(), page: String::new() };
        update(&p, "m", |i| i.manual = vec![hand("../nothing-here"), hand("mods/b.jar")]).unwrap();
        assert_eq!(check_manual(&p, "m").unwrap().manual, vec![hand("mods/b.jar")]);
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
