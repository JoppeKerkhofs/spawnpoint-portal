use serde::{Deserialize, Serialize};

use crate::instance::slug;
use crate::{Error, Paths, Result, secrets};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub address: String,
    pub name: String,
}

pub fn normalize_address(input: &str, allow_http: bool) -> Result<String> {
    let input = input.trim();
    let full = if input.contains("://") { input.to_string() } else { format!("https://{input}") };
    let lower = full.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(Error::Other(format!("{input} is not an http(s) address")));
    }
    let trimmed = full.trim_end_matches('/');
    let address = trimmed.strip_suffix("/api/v1").unwrap_or(trimmed).trim_end_matches('/').to_string();
    if lower.starts_with("http://") && !allow_http {
        return Err(Error::Other(format!("{address} is not https; pass --allow-http to link it anyway")));
    }
    Ok(address)
}

pub fn list(paths: &Paths) -> Result<Vec<Panel>> {
    match std::fs::read(paths.panels_file()) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

fn save(paths: &Paths, panels: &[Panel]) -> Result<()> {
    let file = paths.panels_file();
    std::fs::create_dir_all(&paths.root)?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(panels)?)?;
    std::fs::rename(&tmp, &file)?;
    Ok(())
}

fn account(id: &str) -> String {
    format!("panel:{id}")
}

pub fn add(paths: &Paths, address: &str, name: &str, token: &str) -> Result<Panel> {
    let mut panels = list(paths)?;
    let panel = if let Some(old) = panels.iter_mut().find(|p| p.address == address) {
        old.name = name.to_string();
        old.clone()
    } else {
        let host = address.split("://").nth(1).unwrap_or(address);
        let base = slug(host.split('/').next().unwrap_or(host));
        let mut id = base.clone();
        let mut n = 2;
        while panels.iter().any(|p| p.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let panel = Panel { id, address: address.to_string(), name: name.to_string() };
        panels.push(panel.clone());
        panel
    };
    secrets::set(&account(&panel.id), token)?;
    save(paths, &panels)?;
    Ok(panel)
}

pub fn remove(paths: &Paths, id: &str) -> Result<()> {
    let mut panels = list(paths)?;
    panels.retain(|p| p.id != id);
    save(paths, &panels)?;
    secrets::delete(&account(id))
}

pub fn token(panel: &Panel) -> Result<String> {
    secrets::get(&account(&panel.id))?
        .ok_or_else(|| Error::Other(format!("the panel {} has no token; link it again", panel.name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::tmp_dir;

    #[test]
    fn addresses() {
        assert_eq!(normalize_address(" panel.example.org/ ", false).unwrap(), "https://panel.example.org");
        assert_eq!(normalize_address("https://panel.example.org/api/v1/", false).unwrap(), "https://panel.example.org");
        assert!(normalize_address("http://10.0.0.5:8080", false).is_err());
        assert_eq!(normalize_address("http://10.0.0.5:8080", true).unwrap(), "http://10.0.0.5:8080");
        assert!(normalize_address("ftp://x", true).is_err());
    }

    #[test]
    fn panels_file_has_no_token() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let host = format!("{}.example.org", &secrets::test_account()[5..].replace(':', "-"));
        let address = format!("https://{host}");
        let p = add(&paths, &address, "Home", "tok-123").unwrap();
        assert!(!std::fs::read_to_string(paths.panels_file()).unwrap().contains("tok-123"));
        assert_eq!(token(&p).unwrap(), "tok-123");
        add(&paths, &address, "Home again", "tok-456").unwrap();
        assert_eq!(list(&paths).unwrap().len(), 1);
        remove(&paths, &p.id).unwrap();
        assert!(token(&p).is_err() && list(&paths).unwrap().is_empty());
    }

    #[test]
    fn a_taken_id_is_numbered() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let host = format!("{}.example.org", &secrets::test_account()[5..].replace(':', "-"));
        let a = add(&paths, &format!("https://{host}"), "A", "t").unwrap();
        let b = add(&paths, &format!("http://{host}"), "B", "t").unwrap();
        assert_eq!(b.id, format!("{}-2", a.id));
        remove(&paths, &a.id).unwrap();
        remove(&paths, &b.id).unwrap();
    }
}
