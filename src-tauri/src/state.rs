use portal_core::Paths;
use portal_core::download::{Progress, ProgressTx};
use portal_core::panel::Pairing;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tokio::time::Instant;

pub struct AppState {
    pub client: reqwest::Client,
    pub paths: Paths,
    pub busy: Mutex<HashSet<String>>,
    pub running: Mutex<HashMap<String, u64>>,
    pub pairing: Mutex<Option<Kept>>,
    pub pairings: AtomicU64,
}

pub struct Kept {
    pub address: String,
    pub pairing: Pairing,
    pub allow_http: bool,
    pub number: u64,
}

pub fn is_kept(slot: &Option<Kept>, number: u64) -> bool {
    slot.as_ref().is_some_and(|k| k.number == number)
}

pub fn forget_pairing(slot: &Mutex<Option<Kept>>, number: u64) {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    if is_kept(&slot, number) {
        *slot = None;
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CommandError {
    pub kind: String,
    pub message: String,
}

impl From<portal_core::Error> for CommandError {
    fn from(e: portal_core::Error) -> Self {
        CommandError { kind: e.kind().into(), message: e.to_string() }
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

#[derive(Debug)]
pub struct Claim<'a> {
    busy: &'a Mutex<HashSet<String>>,
    key: String,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.key);
    }
}

pub fn claim<'a>(busy: &'a Mutex<HashSet<String>>, key: &str) -> CmdResult<Claim<'a>> {
    if !busy.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string()) {
        return Err(CommandError { kind: "busy".into(), message: "This instance is busy.".into() });
    }
    Ok(Claim { busy, key: key.to_string() })
}

pub fn refuse_if_running(running: &Mutex<HashMap<String, u64>>, id: &str) -> CmdResult<()> {
    if running.lock().unwrap_or_else(|e| e.into_inner()).contains_key(id) {
        return Err(CommandError { kind: "busy".into(), message: "this instance is running".into() });
    }
    Ok(())
}

struct RunGuard<'a>(&'a Mutex<HashMap<String, u64>>, &'a str);

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(self.1);
    }
}

// Takes the instance out of `running` after `work`, also when `work` panics.
pub fn end_run(running: &Mutex<HashMap<String, u64>>, id: &str, work: impl FnOnce()) {
    let _guard = RunGuard(running, id);
    work();
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Running {
    pub instance: String,
    pub started_at: u64,
}

pub fn list_running(running: &Mutex<HashMap<String, u64>>) -> Vec<Running> {
    let mut list: Vec<Running> = running
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(instance, started_at)| Running { instance: instance.clone(), started_at: *started_at })
        .collect();
    list.sort_by(|a, b| a.instance.cmp(&b.instance));
    list
}

pub fn safe_url(url: &str) -> CmdResult<&str> {
    let refuse = || CommandError { kind: "other".into(), message: "only https pages are opened".into() };
    let rest = url.strip_prefix("https://").ok_or_else(refuse)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = authority.split_once(':').unwrap_or((authority, "0"));
    let plain = host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let port_ok = !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit());
    let clean = !url.chars().any(|c| c == '@' || c.is_whitespace() || c.is_control());
    if clean && plain && port_ok && !host.is_empty() && !host.starts_with(['-', '.']) {
        Ok(url)
    } else {
        Err(refuse())
    }
}

#[derive(Serialize)]
pub struct PairingView {
    pub pairing: u64,
    pub code: String,
    pub address: String,
    pub approve_at: String,
    pub expires_at: String,
    pub interval: u64,
}

impl PairingView {
    pub fn new(address: &str, pairing: &Pairing, number: u64) -> PairingView {
        PairingView {
            pairing: number,
            code: pairing.code.clone(),
            address: address.into(),
            approve_at: format!("{address}/settings/api-tokens"),
            expires_at: pairing.expires_at.clone(),
            interval: pairing.interval,
        }
    }
}

const MIN_GAP_MS: u64 = 100;

// Passes a change of stage or step at once, anything else at most every 100 ms.
#[derive(Default)]
struct Throttle {
    key: Option<(u32, String)>,
    sent_at: Option<u64>,
}

impl Throttle {
    fn admit(&mut self, p: &Progress, now_ms: u64) -> bool {
        let changed = self.key.as_ref().is_none_or(|(s, g)| *s != p.step || *g != p.stage);
        let due = self.sent_at.is_none_or(|t| now_ms.saturating_sub(t) >= MIN_GAP_MS);
        if changed || due {
            self.key = Some((p.step, p.stage.clone()));
            self.sent_at = Some(now_ms);
        }
        changed || due
    }
}

pub fn forward_progress(emit: impl Fn(Progress) + Send + 'static) -> ProgressTx {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    tokio::spawn(async move {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        let mut held: Option<Progress> = None;
        loop {
            let next = if held.is_some() {
                match tokio::time::timeout(Duration::from_millis(MIN_GAP_MS), rx.recv()).await {
                    Ok(next) => next,
                    Err(_) => {
                        let p = held.take().expect("held is set");
                        throttle.admit(&p, start.elapsed().as_millis() as u64);
                        emit(p);
                        continue;
                    }
                }
            } else {
                rx.recv().await
            };
            let Some(p) = next else { break };
            if let Some(h) = held.take_if(|h| (h.step, &h.stage) != (p.step, &p.stage)) {
                emit(h);
            }
            if throttle.admit(&p, start.elapsed().as_millis() as u64) {
                held = None;
                emit(p);
            } else {
                held = Some(p);
            }
        }
        if let Some(p) = held {
            emit(p);
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_second_claim_is_refused_until_the_first_is_dropped() {
        let busy = Mutex::new(HashSet::new());
        let first = claim(&busy, "join:home/atm10").unwrap();
        assert_eq!(claim(&busy, "join:home/atm10").unwrap_err(), CommandError { kind: "busy".into(), message: "This instance is busy.".into() });
        assert!(claim(&busy, "atm10").is_ok());
        drop(first);
        assert!(claim(&busy, "join:home/atm10").is_ok());
    }

    #[test]
    fn a_running_instance_is_refused() {
        let running = Mutex::new(HashMap::from([("atm10".to_string(), 100)]));
        assert_eq!(refuse_if_running(&running, "atm10").unwrap_err(), CommandError { kind: "busy".into(), message: "this instance is running".into() });
        assert!(refuse_if_running(&running, "other").is_ok());
    }

    #[test]
    fn a_run_ends_even_if_recording_it_panics() {
        let running = Mutex::new(HashMap::from([("atm10".to_string(), 100)]));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| end_run(&running, "atm10", || panic!("record failed"))));
        assert!(r.is_err());
        assert!(running.lock().unwrap().is_empty());
    }

    #[test]
    fn running_lists_each_instance_with_its_start_time() {
        let running = Mutex::new(HashMap::from([("b".to_string(), 200), ("a".to_string(), 100)]));
        assert_eq!(list_running(&running), [Running { instance: "a".into(), started_at: 100 }, Running { instance: "b".into(), started_at: 200 }]);
        assert_eq!(serde_json::to_value(&list_running(&running)[0]).unwrap(), serde_json::json!({ "instance": "a", "started_at": 100 }));
    }

    #[test]
    fn only_https_is_opened() {
        assert!(safe_url("https://www.curseforge.com/minecraft/mc-mods/x").is_ok());
        for good in ["https://example.org", "https://example.org:8443/a?b=c#d", "https://www.curseforge.com/minecraft/mc-mods/x"] {
            assert!(safe_url(good).is_ok(), "{good}");
        }
        for bad in ["https://", "https:///x", "https://@evil.org", "https://user@evil.org/", "https://good.org@evil.org", "https://a b.org", "https://a.org/x y", "https://a.org/\u{1}", r"https://a.org\@b.org","https://:443", "https://a.org:", "https://a.org:44x", "https://-a.org", "https://[::1]/", "http://x", "file:///C:/Windows/System32/calc.exe", "javascript:alert(1)", r"C:\x.exe", "", "HTTPS:evil", " https://x"] {
            assert!(safe_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_pairing_view_has_no_secret() {
        let p = Pairing { code: "7ABR547C".into(), secret: "s3cr3t".into(), expires_at: "2026-11-20T14:10:00Z".into(), interval: 3 };
        let json = serde_json::to_string(&PairingView::new("https://panel.example.org", &p, 1)).unwrap();
        assert!(!json.contains("s3cr3t"));
        assert!(json.contains("https://panel.example.org/settings/api-tokens"));
    }

    #[test]
    fn only_the_kept_pairing_number_matches() {
        let pairing = Pairing { code: "7ABR547C".into(), secret: "s3cr3t".into(), expires_at: "2026-11-20T14:10:00Z".into(), interval: 3 };
        let slot = Mutex::new(Some(Kept { address: "https://panel.example.org".into(), pairing, allow_http: false, number: 2 }));
        forget_pairing(&slot, 1);
        assert!(slot.lock().unwrap().is_some());
        assert!(is_kept(&slot.lock().unwrap(), 2) && !is_kept(&slot.lock().unwrap(), 1) && !is_kept(&None, 2));
        forget_pairing(&slot, 2);
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn a_core_error_becomes_kind_and_message() {
        let e = CommandError::from(portal_core::Error::Unreachable { panel: "Home".into() });
        assert_eq!(serde_json::to_value(&e).unwrap(), serde_json::json!({ "kind": "unreachable", "message": "panel Home: cannot be reached" }));
    }

    fn ev(step: u32, stage: &str) -> Progress {
        Progress { step, steps: 5, stage: stage.into(), done: 0, total: 0 }
    }

    #[test]
    fn the_throttle_passes_changes_and_spaces_the_rest() {
        let mut t = Throttle::default();
        // (step, stage, ms, expected)
        let seq = [
            (2, "mods", 0, true),
            (2, "mods", 10, false),
            (2, "mods", 99, false),
            (2, "mods", 100, true),
            (2, "mods", 150, false),
            (2, "starting", 151, true),
            (3, "starting", 152, true),
            (3, "starting", 160, false),
        ];
        for (step, stage, ms, want) in seq {
            assert_eq!(t.admit(&ev(step, stage), ms), want, "{stage} at {ms}");
        }
    }

    #[tokio::test]
    async fn progress_is_forwarded_in_order() {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        let tx = forward_progress(move |p| sink.lock().unwrap().push(p.stage));
        for s in ["a", "b", "c"] {
            tx.send(ev(1, s)).unwrap();
        }
        drop(tx);
        for _ in 0..100 {
            if seen.lock().unwrap().len() == 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(*seen.lock().unwrap(), ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn the_last_event_of_a_stage_survives_the_next_stage() {
        let seen = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
        let sink = seen.clone();
        let tx = forward_progress(move |p| sink.lock().unwrap().push((p.stage, p.done)));
        for done in 1..=1204 {
            tx.send(Progress { done, total: 1204, ..ev(2, "mods") }).unwrap();
        }
        tx.send(ev(3, "java")).unwrap();
        drop(tx);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let seen = seen.lock().unwrap();
        let n = seen.len();
        assert_eq!(seen[n - 2], ("mods".to_string(), 1204));
        assert_eq!(seen[n - 1], ("java".to_string(), 0));
    }

    #[tokio::test]
    async fn the_last_event_of_a_burst_is_forwarded() {
        let seen = Arc::new(Mutex::new(Vec::<u64>::new()));
        let sink = seen.clone();
        let tx = forward_progress(move |p| sink.lock().unwrap().push(p.done));
        for done in 0..50 {
            tx.send(Progress { done, ..ev(2, "mods") }).unwrap();
        }
        drop(tx);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let seen = seen.lock().unwrap();
        assert_eq!(seen.last(), Some(&49));
        assert!(seen.len() < 50);
    }
}
