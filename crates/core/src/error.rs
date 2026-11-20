use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Http(reqwest::Error),
    Status { url: String, status: u16 },
    Json(serde_json::Error),
    Zip(zip::result::ZipError),
    Hash { path: PathBuf, expected: String, actual: String },
    Unsupported(String),
    Secret(String),
    NotReady(String),
    Panel { panel: String, message: String },
    Unreachable { panel: String },
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Http(e) => write!(f, "http error: {e}"),
            Error::Status { url, status } => write!(f, "{url} answered with status {status}"),
            Error::Json(e) => write!(f, "json error: {e}"),
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Hash { path, expected, actual } => write!(
                f,
                "{} has hash {actual}, expected {expected}",
                path.display()
            ),
            Error::Secret(m) => write!(f, "credential store error: {m}"),
            Error::Panel { panel, message } => write!(f, "panel {panel}: {message}"),
            Error::Unreachable { panel } => write!(f, "panel {panel}: cannot be reached"),
            Error::NotReady(m) | Error::Unsupported(m) | Error::Other(m) => f.write_str(m),
        }
    }
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Io(_) => "io",
            Error::Http(_) | Error::Status { .. } => "network",
            Error::Unreachable { .. } => "unreachable",
            Error::Panel { .. } => "panel",
            Error::Json(_) | Error::Zip(_) | Error::Hash { .. } => "data",
            Error::Unsupported(_) => "unsupported",
            Error::NotReady(_) => "not_ready",
            Error::Secret(_) => "secret",
            Error::Other(_) => "other",
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Http(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Zip(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds() {
        assert_eq!(Error::Unreachable { panel: "Home".into() }.kind(), "unreachable");
        assert_eq!(Error::Unreachable { panel: "Home".into() }.to_string(), "panel Home: cannot be reached");
        assert_eq!(Error::NotReady("x".into()).kind(), "not_ready");
        assert_eq!(Error::Other("x".into()).kind(), "other");
    }
}
