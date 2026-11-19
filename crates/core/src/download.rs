use crate::{Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub const CONCURRENT_DOWNLOADS: usize = 16;
pub const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hash {
    Sha1(String),
    Sha512(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub hash: Option<Hash>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
}

pub type ProgressTx = tokio::sync::mpsc::UnboundedSender<Progress>;

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_with<D: Digest>(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = D::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

pub fn sha1_file(path: &Path) -> Result<String> {
    hash_with::<Sha1>(path)
}

pub fn sha512_file(path: &Path) -> Result<String> {
    hash_with::<Sha512>(path)
}

pub fn file_matches(path: &Path, hash: &Hash) -> bool {
    let (expected, actual) = match hash {
        Hash::Sha1(h) => (h, sha1_file(path)),
        Hash::Sha512(h) => (h, sha512_file(path)),
    };
    actual.is_ok_and(|a| a.eq_ignore_ascii_case(expected))
}

fn is_current(item: &Download) -> bool {
    match &item.hash {
        Some(hash) => file_matches(&item.path, hash),
        None => std::fs::metadata(&item.path)
            .is_ok_and(|m| item.size.is_none_or(|s| m.len() == s)),
    }
}

async fn fetch_once(client: &Client, item: &Download) -> Result<()> {
    let mut part = item.path.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    if let Some(dir) = item.path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut resp = client.get(&item.url).send().await?;
    if !resp.status().is_success() {
        return Err(Error::Status { url: item.url.clone(), status: resp.status().as_u16() });
    }
    let mut file = tokio::fs::File::create(&part).await?;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(hash) = &item.hash {
        let (expected, actual) = match hash {
            Hash::Sha1(h) => (h, sha1_file(&part)?),
            Hash::Sha512(h) => (h, sha512_file(&part)?),
        };
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Hash { path: item.path.clone(), expected: expected.clone(), actual });
        }
    }
    tokio::fs::rename(&part, &item.path).await?;
    Ok(())
}

pub async fn fetch(client: &Client, item: &Download) -> Result<()> {
    if is_current(item) {
        return Ok(());
    }
    let mut attempt = 1;
    loop {
        match fetch_once(client, item).await {
            Err(Error::Status { status, .. }) if (400..500).contains(&status) => {
                return Err(Error::Status { url: item.url.clone(), status });
            }
            Err(e) if attempt >= ATTEMPTS => return Err(e),
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
                attempt += 1;
            }
            Ok(()) => return Ok(()),
        }
    }
}

pub async fn fetch_all(
    client: &Client,
    stage: &str,
    items: Vec<Download>,
    progress: &ProgressTx,
) -> Result<()> {
    let total = items.len() as u64;
    let limit = Arc::new(Semaphore::new(CONCURRENT_DOWNLOADS));
    let mut tasks = JoinSet::new();
    for item in items {
        let (client, limit) = (client.clone(), limit.clone());
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await.expect("semaphore is never closed");
            fetch(&client, &item).await
        });
    }
    let mut done = 0;
    while let Some(joined) = tasks.join_next().await {
        joined.map_err(|e| Error::Other(e.to_string()))??;
        done += 1;
        let _ = progress.send(Progress { stage: stage.to_string(), done, total });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "portal-dl-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tmp_with(bytes: &[u8]) -> PathBuf {
        let path = tmp_dir().join("f");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    // Answers every request with `status` and `body`; returns the base url and a request counter.
    async fn serve(status: u16, body: &'static [u8]) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let mut seen = Vec::new();
                    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => seen.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body).await;
                });
            }
        });
        (url, hits)
    }

    fn item(url: String, path: PathBuf, hash: Option<Hash>) -> Download {
        Download { url, path, hash, size: None }
    }

    const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    #[test]
    fn hashes_a_file() {
        assert_eq!(sha1_file(&tmp_with(b"abc")).unwrap(), ABC_SHA1);
    }

    #[tokio::test]
    async fn a_right_file_is_not_fetched_again() {
        let (url, hits) = serve(200, b"abc").await;
        let path = tmp_with(b"abc");
        let it = item(url, path, Some(Hash::Sha1(ABC_SHA1.into())));
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_wrong_file_is_replaced() {
        let (url, hits) = serve(200, b"abc").await;
        let path = tmp_with(b"other");
        let it = item(url, path.clone(), Some(Hash::Sha1(ABC_SHA1.into())));
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_wrong_hash_leaves_no_file() {
        let (url, _) = serve(200, b"abc").await;
        let path = tmp_dir().join("sub").join("f");
        let it = item(url, path.clone(), Some(Hash::Sha1("0".repeat(40))));
        let err = fetch(&http_client(), &it).await.unwrap_err();
        assert!(matches!(err, Error::Hash { .. }));
        assert!(!path.exists());
        assert!(!PathBuf::from(format!("{}.part", path.display())).exists());
    }

    #[tokio::test]
    async fn a_leftover_part_file_is_overwritten() {
        let (url, _) = serve(200, b"abc").await;
        let path = tmp_dir().join("x");
        std::fs::write(format!("{}.part", path.display()), b"stale and longer").unwrap();
        let it = item(url, path.clone(), None);
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    }

    #[tokio::test]
    async fn fetch_all_reports_every_file() {
        let (url, _) = serve(200, b"abc").await;
        let dir = tmp_dir();
        let items = (0..5)
            .map(|i| item(format!("{url}/{i}"), dir.join(i.to_string()), None))
            .collect();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        fetch_all(&http_client(), "files", items, &tx).await.unwrap();
        drop(tx);
        let mut last = None;
        while let Some(p) = rx.recv().await {
            last = Some(p);
        }
        let last = last.unwrap();
        assert_eq!((last.stage.as_str(), last.done, last.total), ("files", 5, 5));
    }

    #[tokio::test]
    async fn a_404_is_not_retried() {
        let (url, hits) = serve(404, b"").await;
        let it = item(url, tmp_dir().join("f"), None);
        let err = fetch(&http_client(), &it).await.unwrap_err();
        assert!(matches!(err, Error::Status { status: 404, .. }));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
