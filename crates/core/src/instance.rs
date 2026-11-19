use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::loader::Loader;
use crate::paths::check_relative;
use crate::{Error, Paths, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub source: Source,
    pub managed: Vec<ManagedFile>,
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
    let folder = dir(paths, &instance.id);
    let tmp = folder.join("instance.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(instance)?)?;
    std::fs::rename(tmp, folder.join("instance.json"))?;
    Ok(())
}

pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
    let entries = match std::fs::read_dir(paths.instances()) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut all = Vec::new();
    for entry in entries {
        if let Some(id) = entry?.file_name().to_str() {
            all.extend(load(paths, id).ok());
        }
    }
    all.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(all)
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
    for entry in std::fs::read_dir(game_dir(paths, &instance.id).join("mods"))? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let Some(name) = entry.file_name().to_str().map(String::from) else { continue };
        if meta.is_file() && (has_ext(&name, ".jar") || has_ext(&name, ".jar.disabled")) {
            let managed = instance.managed.iter().any(|m| m.path == format!("mods/{name}"));
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
    fn source_round_trips() {
        let (_t, p) = tmp();
        let pack = PackRef { catalogue: "ftb".into(), project: "1".into(), version: "2".into() };
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
        assert!(remove_mod(&p, &mut inst, r"..\instance.json").is_err());
        assert!(remove_mod(&p, &mut inst, "../instance.json").is_err());
        assert!(remove_mod(&p, &mut inst, "gone.jar").is_err());
        remove_mod(&p, &mut inst, "own.jar").unwrap();
        remove_mod(&p, &mut inst, "off.jar.disabled").unwrap();
        assert!(mods(&p, &inst).unwrap().is_empty());
        assert!(inst.managed.is_empty());
        assert!(load(&p, "m").unwrap().managed.is_empty());
    }
}
