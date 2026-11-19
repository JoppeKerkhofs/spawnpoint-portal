use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::download::{Hash, file_matches, sha1_file};
use crate::instance::ManagedFile;
use crate::paths::check_relative;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Wanted {
    pub path: String,
    pub from: Origin,
    pub hash: Option<Hash>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum Origin {
    Url(String),
    Bytes(Vec<u8>),
    Manual { name: String, page: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    pub path: String,
    pub name: String,
    pub page: String,
}

#[derive(Debug, Clone)]
pub struct SyncPlan {
    pub write: Vec<Wanted>,
    pub delete: Vec<String>,
    pub managed: Vec<ManagedFile>,
    pub manual: Vec<Manual>,
}

pub fn safe_path(path: &str) -> Result<String> {
    let norm = path.replace('\\', "/");
    let norm = norm.strip_prefix("./").unwrap_or(&norm);
    let bad_segment = norm.split('/').any(|s| s.is_empty() || s == "." || s == "..");
    if norm.contains(':') || bad_segment || check_relative(norm).is_err() {
        return Err(Error::Other(format!("unsafe path in pack: {path}")));
    }
    Ok(norm.to_string())
}

pub fn plan(game_dir: &Path, managed: &[ManagedFile], wanted: Vec<Wanted>) -> Result<SyncPlan> {
    let mut list: Vec<Wanted> = Vec::new();
    for mut w in wanted {
        w.path = safe_path(&w.path)?;
        list.retain(|o| o.path != w.path);
        list.push(w);
    }
    let managed_sha = |p: &str| managed.iter().find(|m| m.path == p).map(|m| m.sha1.as_str());
    let wanted_paths: Vec<String> = list.iter().map(|w| w.path.clone()).collect();
    let mut out = SyncPlan { write: vec![], delete: vec![], managed: vec![], manual: vec![] };

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
            continue;
        }
        let sha1 = sha1_file(&full)?;
        let ours = managed_sha(&w.path).is_some_and(|m| m.eq_ignore_ascii_case(&sha1));
        let right = w.hash.as_ref().is_some_and(|h| file_matches(&full, h));
        let manual = matches!(w.from, Origin::Manual { .. });
        if right || manual || (w.hash.is_none() && ours) {
            out.managed.push(ManagedFile { path: w.path, sha1 });
        } else if ours {
            out.write.push(w);
        }
    }

    for m in managed {
        let Ok(p) = safe_path(&m.path) else { continue };
        let full = game_dir.join(&p);
        if !wanted_paths.contains(&p)
            && full.is_file()
            && sha1_file(&full)?.eq_ignore_ascii_case(&m.sha1)
        {
            out.delete.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::{Tmp, tmp_dir};
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
            p.delete.iter().map(String::as_str).collect(),
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
        for bad in ["", "/etc/x", r"C:\Windows\x", "mods/../../x", "..", r"..\x", "a//b", "a/./b"] {
            assert!(safe_path(bad).is_err(), "{bad}");
        }
        let err = safe_path("../x").unwrap_err().to_string();
        assert_eq!(err, "unsafe path in pack: ../x");
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
}
