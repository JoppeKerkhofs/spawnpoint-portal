use crate::{Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};
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
    Sha256(String),
    Sha512(String),
}

#[derive(Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub hash: Option<Hash>,
    pub size: Option<u64>,
    // Panel token: fetch sends it only through the no-redirect panel client.
    pub bearer: Option<String>,
}

impl std::fmt::Debug for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Download")
            .field("url", &self.url)
            .field("path", &self.path)
            .field("hash", &self.hash)
            .field("size", &self.size)
            .field("bearer", &self.bearer.as_ref().map(|_| "***"))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub step: u32,
    pub steps: u32,
    pub stage: String,
    pub done: u64,
    pub total: u64,
}

pub type ProgressTx = tokio::sync::mpsc::UnboundedSender<Progress>;

// Numbers events by stage order; a stage not found ahead keeps the current step.
pub fn numbered(stages: Vec<&'static str>, out: ProgressTx) -> ProgressTx {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    tokio::spawn(async move {
        let mut step = 0;
        while let Some(mut p) = rx.recv().await {
            let from = step.max(1) as usize - 1;
            if let Some(i) = stages[from..].iter().position(|s| *s == p.stage) {
                step = (from + i + 1) as u32;
            }
            p.step = step;
            p.steps = stages.len() as u32;
            let _ = out.send(p);
        }
    });
    tx
}

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

pub fn sha1_bytes(bytes: &[u8]) -> String {
    hex(Sha1::digest(bytes))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    hash_with::<Sha256>(path)
}

pub fn sha512_file(path: &Path) -> Result<String> {
    hash_with::<Sha512>(path)
}

pub fn file_matches(path: &Path, hash: &Hash) -> bool {
    let (expected, actual) = match hash {
        Hash::Sha1(h) => (h, sha1_file(path)),
        Hash::Sha256(h) => (h, sha256_file(path)),
        Hash::Sha512(h) => (h, sha512_file(path)),
    };
    actual.is_ok_and(|a| a.eq_ignore_ascii_case(expected))
}

pub fn part_path(path: &Path) -> PathBuf {
    let mut part = path.as_os_str().to_owned();
    part.push(".portal-part");
    PathBuf::from(part)
}

// Runs disk-heavy work on a blocking thread; a panic in it comes back as an error.
pub(crate) async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(|e| Error::Other(e.to_string()))?
}

fn is_current(item: &Download) -> bool {
    match &item.hash {
        Some(hash) => file_matches(&item.path, hash),
        None => std::fs::metadata(&item.path)
            .is_ok_and(|m| item.size.is_none_or(|s| m.len() == s)),
    }
}

pub async fn get_ok(client: &Client, url: &str) -> Result<reqwest::Response> {
    get_with(client, url, None).await
}

async fn get_with(client: &Client, url: &str, bearer: Option<&str>) -> Result<reqwest::Response> {
    let mut request = client.get(url);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status { url: url.into(), status: status.as_u16() });
    }
    Ok(response)
}

async fn fetch_once(client: &Client, item: &Download) -> Result<()> {
    let part = part_path(&item.path);
    if let Some(dir) = item.path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let panel;
    let client = if item.bearer.is_some() {
        panel = crate::panel_client();
        &panel
    } else {
        client
    };
    let mut resp = get_with(client, &item.url, item.bearer.as_deref()).await?;
    let mut file = tokio::fs::File::create(&part).await?;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(hash) = &item.hash {
        let (owned, file) = (hash.clone(), part.clone());
        let actual = blocking(move || match owned {
            Hash::Sha1(_) => sha1_file(&file),
            Hash::Sha256(_) => sha256_file(&file),
            Hash::Sha512(_) => sha512_file(&file),
        })
        .await?;
        let (Hash::Sha1(expected) | Hash::Sha256(expected) | Hash::Sha512(expected)) = hash;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Hash { path: item.path.clone(), expected: expected.clone(), actual });
        }
    }
    tokio::fs::rename(&part, &item.path).await?;
    Ok(())
}

pub async fn fetch(client: &Client, item: &Download) -> Result<()> {
    let current = item.clone();
    if blocking(move || Ok(is_current(&current))).await? {
        return Ok(());
    }
    let mut attempt = 1;
    loop {
        match fetch_once(client, item).await {
            Err(Error::Status { status, .. }) if (300..500).contains(&status) => {
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
    let _ = progress.send(Progress { step: 0, steps: 0, stage: stage.to_string(), done: 0, total });
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
    let mut first_error = None;
    while let Some(joined) = tasks.join_next().await {
        match joined.map_err(|e| Error::Other(e.to_string())).and_then(|r| r) {
            Ok(()) => {
                done += 1;
                let _ = progress.send(Progress { step: 0, steps: 0, stage: stage.to_string(), done, total });
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub struct Tmp(PathBuf);

    impl std::ops::Deref for Tmp {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn tmp_dir() -> Tmp {
        let dir = std::env::temp_dir().join(format!(
            "portal-dl-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    // Answers every request with `status` and `body`; returns the base url and a request counter.
    pub async fn serve(status: u16, body: &'static [u8]) -> (String, Arc<AtomicUsize>) {
        let (url, hits, _) = serve_full(status, body.to_vec(), String::new()).await;
        (url, hits)
    }

    // Like `serve`, with extra raw header lines and the raw head of every request seen.
    pub async fn serve_full(
        status: u16,
        body: Vec<u8>,
        extra: String,
    ) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>) {
        serve_routes(vec![("", status, body, extra)]).await
    }

    // The first route whose key is part of the request line answers; no match gives 404.
    pub async fn serve_routes(
        routes: Vec<(&'static str, u16, Vec<u8>, String)>,
    ) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let heads = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (counter, recorded) = (hits.clone(), heads.clone());
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let (routes, recorded) = (routes.clone(), recorded.clone());
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let mut seen = Vec::new();
                    let end = loop {
                        if let Some(i) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
                            break i + 4;
                        }
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => seen.extend_from_slice(&buf[..n]),
                        }
                    };
                    let head = String::from_utf8_lossy(&seen[..end]).to_string();
                    let length = head
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    while seen.len() < end + length {
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => seen.extend_from_slice(&buf[..n]),
                        }
                    }
                    recorded.lock().unwrap().push(head.clone());
                    let line = head.lines().next().unwrap_or_default();
                    let none = ("", 404, Vec::new(), String::new());
                    let (_, status, body, extra) = routes.iter().find(|r| line.contains(r.0)).unwrap_or(&none);
                    let head = format!(
                        "HTTP/1.1 {status} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body).await;
                });
            }
        });
        (url, hits, heads)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client;
    use crate::download::testing::{Tmp, serve, serve_full, tmp_dir};
    use std::sync::atomic::Ordering;

    fn tmp_with(bytes: &[u8]) -> Tmp {
        let dir = tmp_dir();
        std::fs::write(dir.join("f"), bytes).unwrap();
        dir
    }

    fn item(url: String, path: PathBuf, hash: Option<Hash>) -> Download {
        Download { url, path, hash, size: None, bearer: None }
    }

    const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    #[test]
    fn hashes_bytes() {
        assert_eq!(sha1_bytes(b"abc"), ABC_SHA1);
    }

    #[test]
    fn hashes_a_file() {
        let dir = tmp_with(b"abc");
        assert_eq!(sha1_file(&dir.join("f")).unwrap(), ABC_SHA1);
    }

    #[tokio::test]
    async fn a_right_file_is_not_fetched_again() {
        let (url, hits) = serve(200, b"abc").await;
        let dir = tmp_with(b"abc");
        let path = dir.join("f");
        let it = item(url, path, Some(Hash::Sha1(ABC_SHA1.into())));
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_wrong_file_is_replaced() {
        let (url, hits) = serve(200, b"abc").await;
        let dir = tmp_with(b"other");
        let path = dir.join("f");
        let it = item(url, path.clone(), Some(Hash::Sha1(ABC_SHA1.into())));
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_wrong_hash_leaves_no_file() {
        let (url, _) = serve(200, b"abc").await;
        let dir = tmp_dir();
        let path = dir.join("sub").join("f");
        let it = item(url, path.clone(), Some(Hash::Sha1("0".repeat(40))));
        let err = fetch(&http_client(), &it).await.unwrap_err();
        assert!(matches!(err, Error::Hash { .. }));
        assert!(!path.exists());
        assert!(!part_path(&path).exists());
    }

    #[tokio::test]
    async fn a_leftover_part_file_is_overwritten() {
        let (url, _) = serve(200, b"abc").await;
        let dir = tmp_dir();
        let path = dir.join("x");
        std::fs::write(part_path(&path), b"stale and longer").unwrap();
        let it = item(url, path.clone(), None);
        fetch(&http_client(), &it).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    }

    #[tokio::test]
    async fn a_players_part_file_survives() {
        let (url, _) = serve(200, b"abc").await;
        let dir = tmp_dir();
        let path = dir.join("x");
        std::fs::write(dir.join("x.part"), b"mine").unwrap();
        fetch(&http_client(), &item(url, path, None)).await.unwrap();
        assert_eq!(std::fs::read(dir.join("x.part")).unwrap(), b"mine");
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
    async fn numbering_moves_forward_only() {
        let (out, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tx = numbered(vec!["check", "mods", "java", "libraries", "assets"], out);
        for s in ["check", "mods", "java", "libraries", "assets", "libraries", "starting"] {
            tx.send(Progress { step: 0, steps: 0, stage: s.into(), done: 0, total: 0 }).unwrap();
        }
        drop(tx);
        let mut got = vec![];
        while let Some(p) = rx.recv().await {
            got.push((p.step, p.steps));
        }
        assert_eq!(got, [(1, 5), (2, 5), (3, 5), (4, 5), (5, 5), (5, 5), (5, 5)]);
    }

    #[tokio::test]
    async fn fetch_all_announces_a_stage_before_its_files() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        fetch_all(&http_client(), "mods", vec![], &tx).await.unwrap();
        drop(tx);
        let mut got = vec![];
        while let Some(p) = rx.recv().await {
            got.push((p.stage, p.done, p.total));
        }
        assert_eq!(got, [("mods".to_string(), 0, 0)]);
    }

    #[tokio::test]
    async fn blocking_returns_the_work_and_its_errors() {
        assert_eq!(blocking(|| Ok(7)).await.unwrap(), 7);
        assert!(matches!(blocking(|| -> Result<()> { Err(Error::Other("x".into())) }).await, Err(Error::Other(_))));
        assert!(blocking(|| -> Result<()> { panic!("boom") }).await.is_err());
    }

    #[tokio::test]
    async fn a_404_is_not_retried() {
        let (url, hits) = serve(404, b"").await;
        let dir = tmp_dir();
        let it = item(url, dir.join("f"), None);
        let err = fetch(&http_client(), &it).await.unwrap_err();
        assert!(matches!(err, Error::Status { status: 404, .. }));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_bearer_is_sent_only_when_set() {
        let (url, _, heads) = serve_full(200, b"abc".to_vec(), String::new()).await;
        let dir = tmp_dir();
        let with = Download { bearer: Some("t".into()), ..item(url.clone(), dir.join("a"), None) };
        fetch(&http_client(), &with).await.unwrap();
        fetch(&http_client(), &item(url, dir.join("b"), None)).await.unwrap();
        let heads = heads.lock().unwrap();
        assert!(heads[0].to_lowercase().contains("authorization: bearer t"));
        assert!(!heads[1].to_lowercase().contains("authorization"));
        assert!(format!("{with:?}").contains("Some(\"***\")"));
    }

    #[tokio::test]
    async fn a_bearer_never_follows_a_redirect_whatever_the_client() {
        for status in [302, 307] {
            let (other, other_hits) = serve(200, b"abc").await;
            let (url, _, _) = serve_full(status, vec![], format!("Location: {other}/x\r\n")).await;
            let dir = tmp_dir();
            let it = Download { bearer: Some("t".into()), ..item(url, dir.join("a"), None) };
            assert!(fetch(&http_client(), &it).await.is_err());
            assert_eq!(other_hits.load(Ordering::SeqCst), 0);
            assert!(!dir.join("a").exists());
        }
    }
}
