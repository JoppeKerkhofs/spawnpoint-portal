use crate::{Error, Result};

const SERVICE: &str = "SpawnpointPortal";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(|e| Error::Secret(e.to_string()))
}

pub fn set(account: &str, secret: &str) -> Result<()> {
    entry(account)?.set_password(secret).map_err(|e| Error::Secret(e.to_string()))
}

pub fn get(account: &str) -> Result<Option<String>> {
    match entry(account)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

pub fn delete(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

#[cfg(test)]
pub fn test_account() -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("test:{nanos}-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip() {
        let account = test_account();
        assert_eq!(get(&account).unwrap(), None);
        set(&account, "s3cret").unwrap();
        assert_eq!(get(&account).unwrap().as_deref(), Some("s3cret"));
        delete(&account).unwrap();
        assert_eq!(get(&account).unwrap(), None);
        delete(&account).unwrap();
    }
}
