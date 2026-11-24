use crate::instance::Instance;
use crate::loader::LoaderKind;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

// The launcher's Discord application id (public, not a secret); empty turns the feature off.
pub const CLIENT_ID: &str = "1573391772867104859";

const MAX_FRAME: usize = 64 * 1024;
const MAX_TEXT: usize = 128;

pub fn frame(opcode: u32, json: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + json.len());
    out.extend(opcode.to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(json.as_bytes());
    out
}

pub fn read_frame(r: &mut impl Read) -> io::Result<(u32, Vec<u8>)> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head)?;
    let opcode = u32::from_le_bytes(head[..4].try_into().expect("4 bytes"));
    let len = u32::from_le_bytes(head[4..].try_into().expect("4 bytes")) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok((opcode, body))
}

// Discord's limit is 128 bytes per text field.
fn cut(mut s: String) -> String {
    let mut n = MAX_TEXT.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    s.truncate(n);
    s
}

pub fn menu() -> Value {
    json!({ "details": "In menu", "assets": { "large_image": "logo", "large_text": "Spawnpoint Portal" } })
}

pub fn activity(instance: &Instance, started_at: u64) -> Value {
    let mut state = format!("Minecraft {}", instance.minecraft);
    if let Some(l) = &instance.loader {
        let name = match l.kind {
            LoaderKind::Fabric => "Fabric",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::Forge => "Forge",
            LoaderKind::Horizon => "Horizon",
        };
        state = format!("{state} \u{b7} {name}");
    }
    json!({
        "details": cut(format!("Playing {}", instance.name)),
        "state": cut(state),
        "timestamps": { "start": started_at },
        "assets": { "large_image": "logo", "large_text": "Spawnpoint Portal" }
    })
}

#[derive(Default)]
pub struct Presence {
    pipe: Option<Timed<Ipc>>,
    nonce: u64,
}

impl Presence {
    // Blocks on the pipe: call from a blocking thread. Any failure drops the connection silently.
    pub fn set(&mut self, activity: Option<Value>) {
        self.set_with(CLIENT_ID, activity);
    }

    fn set_with(&mut self, client_id: &str, activity: Option<Value>) {
        if client_id.is_empty() {
            return;
        }
        if self.pipe.is_none() && activity.is_none() {
            return;
        }
        // A pipe left over from before Discord restarted fails on send: connect afresh once.
        for _ in 0..2 {
            if self.pipe.is_none() {
                self.pipe = connect(client_id);
            }
            let Some(pipe) = &mut self.pipe else { return };
            self.nonce += 1;
            match send(pipe, std::process::id(), self.nonce, activity.clone()) {
                Ok(()) => return,
                Err(e) => {
                    self.pipe = None;
                    if e.kind() == io::ErrorKind::TimedOut {
                        return;
                    }
                }
            }
        }
    }
}

const ANSWER_WITHIN: Duration = Duration::from_secs(5);

trait Link: Write {
    type Reader: Read + Send + 'static;
    fn reader(&self) -> io::Result<Self::Reader>;
}

#[cfg(windows)]
type Ipc = std::fs::File;
#[cfg(unix)]
type Ipc = std::os::unix::net::UnixStream;

impl Link for Ipc {
    type Reader = Ipc;
    fn reader(&self) -> io::Result<Ipc> {
        self.try_clone()
    }
}

// One helper thread reads one answer, started after the write, so no read is pending during a write.
// A thread stuck on a dead pipe is left to end when Discord closes it; the connection is dropped.
struct Timed<L> {
    link: L,
    timeout: Duration,
}

trait Peer {
    fn send_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn answer(&mut self) -> io::Result<(u32, Vec<u8>)>;
}

impl<T: Read + Write> Peer for T {
    fn send_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }
    fn answer(&mut self) -> io::Result<(u32, Vec<u8>)> {
        read_frame(self)
    }
}

impl<L: Link> Peer for Timed<L> {
    fn send_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.link.write_all(bytes)
    }
    fn answer(&mut self) -> io::Result<(u32, Vec<u8>)> {
        let mut reader = self.link.reader()?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_frame(&mut reader));
        });
        match rx.recv_timeout(self.timeout) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => Err(io::ErrorKind::TimedOut.into()),
            Err(RecvTimeoutError::Disconnected) => Err(io::ErrorKind::UnexpectedEof.into()),
        }
    }
}

fn connect(client_id: &str) -> Option<Timed<Ipc>> {
    places(cfg!(windows), |name| std::env::var(name).ok()).into_iter().find_map(|place| {
        let mut link = Timed { link: open(&place).ok()?, timeout: ANSWER_WITHIN };
        handshake(&mut link, client_id).ok()?;
        Some(link)
    })
}

#[cfg(windows)]
fn open(place: &Path) -> io::Result<Ipc> {
    std::fs::OpenOptions::new().read(true).write(true).open(place)
}

#[cfg(unix)]
fn open(place: &Path) -> io::Result<Ipc> {
    Ipc::connect(place)
}

// Where Discord may listen: a named pipe on Windows, elsewhere a socket file in the runtime
// folder, which a Flatpak or Snap Discord keeps in a folder of its own.
fn places(windows: bool, var: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    if windows {
        return (0..10).map(|n| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{n}"))).collect();
    }
    let runtime = PathBuf::from(var("XDG_RUNTIME_DIR").or_else(|| var("TMPDIR")).unwrap_or_else(|| "/tmp".into()));
    let folders = [runtime.clone(), runtime.join("app/com.discordapp.Discord"), runtime.join("snap.discord")];
    folders.iter().flat_map(|f| (0..10).map(move |n| f.join(format!("discord-ipc-{n}")))).collect()
}

fn handshake(pipe: &mut impl Peer, client_id: &str) -> io::Result<()> {
    pipe.send_all(&frame(0, &json!({ "v": 1, "client_id": client_id }).to_string()))?;
    let (opcode, body) = pipe.answer()?;
    let ready = opcode == 1 && serde_json::from_slice::<Value>(&body).is_ok_and(|v| v["evt"] == "READY");
    if ready { Ok(()) } else { Err(io::Error::other("no READY from Discord")) }
}

fn send(pipe: &mut impl Peer, pid: u32, nonce: u64, activity: Option<Value>) -> io::Result<()> {
    let cmd = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": pid, "activity": activity }, "nonce": nonce.to_string() });
    pipe.send_all(&frame(1, &cmd.to_string()))?;
    match pipe.answer()? {
        (2, _) => Err(io::Error::other("Discord closed the pipe")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Loader;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{Arc, Mutex};

    fn inst(name: &str, loader: Option<LoaderKind>) -> Instance {
        let mut i: Instance = serde_json::from_value(json!({
            "id": "x", "name": name, "minecraft": "1.21.1", "loader": null,
            "source": { "kind": "manual" }, "managed": []
        }))
        .unwrap();
        i.loader = loader.map(|kind| Loader { kind, version: Some("1".into()) });
        i
    }

    struct Fake {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Fake {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.input.read(b)
        }
    }
    impl Write for Fake {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.output.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_frame_is_opcode_length_and_body_little_endian() {
        assert_eq!(frame(1, "{}"), [1, 0, 0, 0, 2, 0, 0, 0, b'{', b'}']);
    }

    #[test]
    fn a_frame_is_read_back_and_bad_ones_are_refused() {
        let (op, body) = read_frame(&mut Cursor::new(frame(3, "[1]"))).unwrap();
        assert_eq!((op, body), (3, b"[1]".to_vec()));
        let mut cut = frame(1, "{\"a\":1}");
        cut.pop();
        assert!(read_frame(&mut Cursor::new(cut)).is_err());
        let mut big = vec![1, 0, 0, 0];
        big.extend((64 * 1024u32 + 1).to_le_bytes());
        assert!(read_frame(&mut Cursor::new(big)).is_err());
        let mut ok = vec![1, 0, 0, 0];
        ok.extend((64 * 1024u32).to_le_bytes());
        ok.extend(vec![b' '; 64 * 1024]);
        assert!(read_frame(&mut Cursor::new(ok)).is_ok());
    }

    #[test]
    fn the_activity_names_the_instance_version_and_loader_only() {
        let a = activity(&inst("Atm10", Some(LoaderKind::NeoForge)), 1700);
        assert_eq!(
            a,
            json!({
                "details": "Playing Atm10",
                "state": "Minecraft 1.21.1 · NeoForge",
                "timestamps": { "start": 1700 },
                "assets": { "large_image": "logo", "large_text": "Spawnpoint Portal" }
            })
        );
        assert_eq!(activity(&inst("Atm10", None), 1)["state"], "Minecraft 1.21.1");
        assert_eq!(activity(&inst("a", Some(LoaderKind::Fabric)), 1)["state"], "Minecraft 1.21.1 · Fabric");
        assert_eq!(activity(&inst("a", Some(LoaderKind::Forge)), 1)["state"], "Minecraft 1.21.1 · Forge");
    }

    #[test]
    fn a_long_name_is_cut_on_a_character_boundary() {
        let d = activity(&inst(&"é".repeat(200), None), 1)["details"].as_str().unwrap().to_string();
        assert!(d.len() <= 128 && d.starts_with("Playing éé"));
        assert!(d.ends_with('é'));
    }

    #[test]
    fn the_handshake_comes_before_the_activity() {
        let mut input = frame(1, r#"{"cmd":"DISPATCH","evt":"READY"}"#);
        input.extend(frame(1, r#"{"cmd":"SET_ACTIVITY"}"#));
        let mut fake = Fake { input: Cursor::new(input), output: vec![] };
        handshake(&mut fake, "123").unwrap();
        send(&mut fake, 42, 7, Some(json!({"details": "Playing x"}))).unwrap();
        let mut out = Cursor::new(fake.output);
        let (op0, b0) = read_frame(&mut out).unwrap();
        assert_eq!((op0, serde_json::from_slice::<Value>(&b0).unwrap()), (0, json!({"v": 1, "client_id": "123"})));
        let (op1, b1) = read_frame(&mut out).unwrap();
        let v: Value = serde_json::from_slice(&b1).unwrap();
        assert_eq!(op1, 1);
        assert_eq!((v["cmd"].as_str(), v["args"]["pid"].as_u64(), v["nonce"].as_str()), (Some("SET_ACTIVITY"), Some(42), Some("7")));
        assert_eq!(v["args"]["activity"]["details"], "Playing x");
    }

    #[test]
    fn a_handshake_without_ready_fails_and_clearing_sends_null() {
        let mut fake = Fake { input: Cursor::new(frame(2, r#"{"code":4000}"#)), output: vec![] };
        assert!(handshake(&mut fake, "123").is_err());
        let mut fake = Fake { input: Cursor::new(frame(1, "{}")), output: vec![] };
        send(&mut fake, 1, 1, None).unwrap();
        let (_, b) = read_frame(&mut Cursor::new(fake.output)).unwrap();
        assert!(serde_json::from_slice::<Value>(&b).unwrap()["args"]["activity"].is_null());
    }

    #[test]
    fn with_no_application_id_set_does_nothing() {
        let mut p = Presence::default();
        p.set_with("", Some(json!({})));
        p.set_with("", None);
        assert!(p.pipe.is_none() && p.nonce == 0);
    }

    // Stands in for the pipe: answers each written frame, and notes a write that starts during a read.
    struct Duplex {
        pending: Arc<AtomicUsize>,
        overlapped: Arc<AtomicBool>,
        replies: bool,
        tx: Sender<Vec<u8>>,
        rx: Arc<Mutex<Receiver<Vec<u8>>>>,
    }
    struct DuplexReader {
        pending: Arc<AtomicUsize>,
        rx: Arc<Mutex<Receiver<Vec<u8>>>>,
        left: Vec<u8>,
    }
    impl Duplex {
        fn new(replies: bool) -> Self {
            let (tx, rx) = channel();
            Duplex { pending: Default::default(), overlapped: Default::default(), replies, tx, rx: Arc::new(Mutex::new(rx)) }
        }
    }
    impl Write for Duplex {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.pending.load(Ordering::SeqCst) > 0 {
                self.overlapped.store(true, Ordering::SeqCst);
            }
            if self.replies {
                let opcode = if b[0] == 0 { r#"{"cmd":"DISPATCH","evt":"READY"}"# } else { "{}" };
                self.tx.send(frame(1, opcode)).unwrap();
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Read for DuplexReader {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            if self.left.is_empty() {
                self.pending.fetch_add(1, Ordering::SeqCst);
                let got = self.rx.lock().unwrap().recv_timeout(Duration::from_secs(2));
                self.pending.fetch_sub(1, Ordering::SeqCst);
                self.left = got.map_err(|_| io::Error::other("closed"))?;
            }
            let n = b.len().min(self.left.len());
            b[..n].copy_from_slice(&self.left[..n]);
            self.left.drain(..n);
            Ok(n)
        }
    }
    impl Link for Duplex {
        type Reader = DuplexReader;
        fn reader(&self) -> io::Result<DuplexReader> {
            Ok(DuplexReader { pending: self.pending.clone(), rx: self.rx.clone(), left: vec![] })
        }
    }

    #[test]
    fn a_discord_that_never_answers_is_given_up_on() {
        let mut link = Timed { link: Duplex::new(false), timeout: Duration::from_millis(50) };
        link.send_all(b"x").unwrap();
        assert_eq!(link.answer().unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn a_discord_that_answers_is_read_through_the_wrapper() {
        let mut link = Timed { link: Duplex::new(true), timeout: Duration::from_secs(5) };
        link.send_all(&frame(1, "{}")).unwrap();
        assert_eq!(link.answer().unwrap(), (1, b"{}".to_vec()));
    }

    #[test]
    fn no_read_is_pending_when_a_write_starts() {
        let duplex = Duplex::new(true);
        let overlapped = duplex.overlapped.clone();
        let mut link = Timed { link: duplex, timeout: Duration::from_secs(5) };
        handshake(&mut link, "123").unwrap();
        send(&mut link, 1, 1, Some(json!({}))).unwrap();
        send(&mut link, 1, 2, None).unwrap();
        assert!(!overlapped.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_file_is_shaken_hands_with() {
        let dir = crate::download::testing::tmp_dir();
        let place = dir.join("discord-ipc-0");
        let listener = std::os::unix::net::UnixListener::bind(&place).unwrap();
        let discord = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let hello = read_frame(&mut peer).unwrap();
            peer.write_all(&frame(1, r#"{"cmd":"DISPATCH","evt":"READY"}"#)).unwrap();
            hello
        });
        let mut link = Timed { link: open(&place).unwrap(), timeout: Duration::from_secs(5) };
        handshake(&mut link, "123").unwrap();
        let (opcode, body) = discord.join().unwrap();
        assert_eq!((opcode, serde_json::from_slice::<Value>(&body).unwrap()), (0, json!({"v": 1, "client_id": "123"})));
    }

    #[test]
    fn discord_is_looked_for_where_each_system_keeps_it() {
        let none = |_: &str| None;
        let pipes = places(true, none);
        assert_eq!((pipes.len(), pipes[3].to_str()), (10, Some("\\\\.\\pipe\\discord-ipc-3")));
        let run = |name: &str| (name == "XDG_RUNTIME_DIR").then(|| "/run/user/1000".to_string());
        let sockets = places(false, run);
        assert_eq!(sockets.len(), 30);
        for (at, want) in [(0, "/run/user/1000/discord-ipc-0"), (13, "/run/user/1000/app/com.discordapp.Discord/discord-ipc-3"), (29, "/run/user/1000/snap.discord/discord-ipc-9")] {
            assert_eq!(sockets[at], Path::new(want));
        }
        assert_eq!(places(false, none)[0], Path::new("/tmp/discord-ipc-0"));
    }
}
