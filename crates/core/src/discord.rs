use crate::instance::Instance;
use crate::loader::LoaderKind;
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};

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
    pipe: Option<File>,
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
            if send(pipe, std::process::id(), self.nonce, activity.clone()).is_ok() {
                return;
            }
            self.pipe = None;
        }
    }
}

fn connect(client_id: &str) -> Option<File> {
    // ponytail: reads have no timeout; a Discord that accepts the pipe and never answers stalls
    // status updates (never the game). Add a reader thread with a timeout if that is ever seen.
    (0..10).find_map(|n| {
        let mut pipe = OpenOptions::new().read(true).write(true).open(pipe_path(n)).ok()?;
        handshake(&mut pipe, client_id).ok()?;
        Some(pipe)
    })
}

fn pipe_path(n: u32) -> String {
    format!(r"\\.\pipe\discord-ipc-{n}")
}

fn handshake(pipe: &mut (impl Read + Write), client_id: &str) -> io::Result<()> {
    pipe.write_all(&frame(0, &json!({ "v": 1, "client_id": client_id }).to_string()))?;
    let (opcode, body) = read_frame(pipe)?;
    let ready = opcode == 1 && serde_json::from_slice::<Value>(&body).is_ok_and(|v| v["evt"] == "READY");
    if ready { Ok(()) } else { Err(io::Error::other("no READY from Discord")) }
}

fn send(pipe: &mut (impl Read + Write), pid: u32, nonce: u64, activity: Option<Value>) -> io::Result<()> {
    let cmd = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": pid, "activity": activity }, "nonce": nonce.to_string() });
    pipe.write_all(&frame(1, &cmd.to_string()))?;
    match read_frame(pipe)? {
        (2, _) => Err(io::Error::other("Discord closed the pipe")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Loader;
    use std::io::Cursor;

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

    #[test]
    fn the_pipe_is_a_device_path() {
        assert_eq!(pipe_path(3), "\\\\.\\pipe\\discord-ipc-3");
    }
}
