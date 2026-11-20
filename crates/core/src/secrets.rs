use crate::{Error, Result};

const SERVICE: &str = "SpawnpointPortal";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(|e| Error::Secret(e.to_string()))
}

fn real_set(account: &str, secret: &str) -> Result<()> {
    entry(account)?.set_password(secret).map_err(|e| Error::Secret(e.to_string()))
}

fn real_get(account: &str) -> Result<Option<String>> {
    match entry(account)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

fn real_delete(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

// Tests use a map so they never touch the real Credential Manager.
#[cfg(test)]
static STORE: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub fn set(account: &str, secret: &str) -> Result<()> {
    STORE.lock().unwrap().get_or_insert_default().insert(account.into(), secret.into());
    Ok(())
}

#[cfg(test)]
pub fn get(account: &str) -> Result<Option<String>> {
    Ok(STORE.lock().unwrap().get_or_insert_default().get(account).cloned())
}

#[cfg(test)]
pub fn delete(account: &str) -> Result<()> {
    STORE.lock().unwrap().get_or_insert_default().remove(account);
    Ok(())
}

#[cfg(not(test))]
pub fn set(account: &str, secret: &str) -> Result<()> {
    real_set(account, secret)
}

#[cfg(not(test))]
pub fn get(account: &str) -> Result<Option<String>> {
    real_get(account)
}

#[cfg(not(test))]
pub fn delete(account: &str) -> Result<()> {
    real_delete(account)
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
    fn the_test_store_round_trips() {
        let account = test_account();
        assert_eq!(get(&account).unwrap(), None);
        set(&account, "s3cret").unwrap();
        assert_eq!(get(&account).unwrap().as_deref(), Some("s3cret"));
        delete(&account).unwrap();
        assert_eq!(get(&account).unwrap(), None);
        delete(&account).unwrap();
    }

    #[test]
    #[ignore = "writes to the real Credential Manager; run by hand"]
    fn the_credential_manager_round_trips() {
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = real_delete(&self.0);
            }
        }
        let account = test_account();
        let _cleanup = Cleanup(account.clone());
        assert_eq!(real_get(&account).unwrap(), None);
        real_set(&account, "s3cret").unwrap();
        assert_eq!(real_get(&account).unwrap().as_deref(), Some("s3cret"));
        real_delete(&account).unwrap();
        assert_eq!(real_get(&account).unwrap(), None);
        real_delete(&account).unwrap();
    }
}
