use crate::loader::Loader;
use crate::sync::Wanted;
use crate::{Error, Result};

pub mod ftb;

#[derive(Debug, Clone)]
pub struct PackInstall {
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub files: Vec<Wanted>,
}

// For anything from the caller that ends up in a URL or one of our own file names.
pub(crate) fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        return Err(Error::Other(format!("{id:?} is not a valid pack id")));
    }
    Ok(())
}

pub(crate) fn https_url(url: &str, name: &str) -> Result<String> {
    if !url.starts_with("https://") {
        return Err(Error::Other(format!("the pack lists a download that is not https: {name}")));
    }
    Ok(url.to_string())
}
