//! Explicit, authenticated loopback transport. Domain state stays on the App thread.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_REQUEST: usize = 64 * 1024;
pub const MAX_RESPONSE: usize = 2 * 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Status,
    Pause,
    Resume,
    Breakpoints { frames: Vec<String> },
    Advance,
    Choose { option: String },
    Reveal,
    SkipTransition,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: u64,
    token: String,
    expected_revision: Option<u64>,
    command: Command,
}
pub struct Job {
    pub command: Command,
    pub expected_revision: Option<u64>,
    pub deadline: Instant,
    reply: SyncSender<std::result::Result<Value, String>>,
}
impl Job {
    pub fn respond(self, result: std::result::Result<Value, String>) {
        let _ = self.reply.try_send(result);
    }
}
pub struct Server {
    pub address: SocketAddr,
    receiver: Receiver<Job>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
pub fn token_file(path: &std::path::Path) -> Result<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(130)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() < 130,
        "{}",
        crate::i18n::format("host-debug-0001", &[])
    );
    let token = String::from_utf8(bytes)?;
    let token = token.trim_end_matches(['\r', '\n']).to_owned();
    validate_token(&token)?;
    Ok(token)
}
fn validate_token(token: &str) -> Result<()> {
    ensure!(
        (32..=128).contains(&token.len())
            && token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{}",
        crate::i18n::format("host-debug-0002", &[])
    );
    Ok(())
}
fn authenticated(expected: &str, actual: &str) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for i in 0..128 {
        difference |= usize::from(
            expected.as_bytes().get(i).copied().unwrap_or(0)
                ^ actual.as_bytes().get(i).copied().unwrap_or(0),
        );
    }
    difference == 0
}
fn response(id: u64, result: std::result::Result<Value, String>) -> Vec<u8> {
    let value = match result {
        Ok(value) => serde_json::json!({"version":1,"id":id,"ok":true,"result":value}),
        Err(message) => serde_json::json!({"version":1,"id":id,"ok":false,"error":message}),
    };
    let mut bytes = serde_json::to_vec(&value).expect("JSON value");
    if bytes.len() > MAX_RESPONSE {
        bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"id":id,"ok":false,"error":"response budget exceeded"}),
        )
        .expect("budget error JSON");
    }
    bytes.push(b'\n');
    bytes
}
struct Transport {
    sender: SyncSender<Job>,
    token: String,
    stop: Arc<AtomicBool>,
    highest: u64,
    recent: BTreeMap<u64, ([u8; 32], Vec<u8>)>,
    order: VecDeque<u64>,
}
impl Transport {
    fn request(&mut self, bytes: &[u8]) -> Vec<u8> {
        // Serde's internally tagged unit variants do not reject unused payload fields.
        let shape = serde_json::from_slice::<Value>(bytes).ok();
        let valid = shape
            .as_ref()
            .and_then(|v| v.get("command"))
            .and_then(Value::as_object)
            .is_some_and(|fields| {
                let extra = match fields.get("type").and_then(Value::as_str) {
                    Some("breakpoints") => Some("frames"),
                    Some("choose") => Some("option"),
                    _ => None,
                };
                fields
                    .keys()
                    .all(|key| key == "type" || Some(key.as_str()) == extra)
            });
        if !valid {
            return response(0, Err("invalid request".into()));
        }
        let request: Request = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(_) => return response(0, Err("invalid request".into())),
        };
        if !authenticated(&self.token, &request.token) {
            return response(request.id, Err("unauthorized".into()));
        }
        if request.version != 1 || request.id == 0 {
            return response(request.id, Err("unsupported version/id".into()));
        }
        let hash = Sha256::digest(bytes).into();
        if let Some((old, reply)) = self.recent.get(&request.id) {
            return if old == &hash {
                reply.clone()
            } else {
                response(request.id, Err("request id conflict".into()))
            };
        }
        if request.id <= self.highest {
            return response(
                request.id,
                Err("expired request id; query status with a new id".into()),
            );
        }
        self.highest = request.id;
        let (tx, rx) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let job = Job {
            command: request.command,
            expected_revision: request.expected_revision,
            deadline,
            reply: tx,
        };
        let result = if self.sender.try_send(job).is_err() {
            Err("debug queue unavailable".into())
        } else {
            loop {
                if self.stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    break Err("debug timeout; query status before retrying".into());
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(result) => break result,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break Err("player disconnected".into());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        };
        let reply = response(request.id, result);
        self.order.push_back(request.id);
        self.recent.insert(request.id, (hash, reply.clone()));
        if self.order.len() > 8 {
            self.recent
                .remove(&self.order.pop_front().expect("cache entry"));
        }
        reply
    }
    fn connection(&mut self, mut stream: TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut input = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut activity = Instant::now();
        while !self.stop.load(Ordering::Relaxed) && activity.elapsed() < Duration::from_secs(30) {
            match stream.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    for byte in &buffer[..n] {
                        if *byte == b'\n' {
                            stream.write_all(&self.request(&input))?;
                            input.clear();
                            activity = Instant::now();
                        } else {
                            input.push(*byte);
                            if input.len() > MAX_REQUEST {
                                stream.write_all(&response(
                                    0,
                                    Err("request budget exceeded".into()),
                                ))?;
                                return Ok(());
                            }
                        }
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
impl Server {
    pub fn bind(address: SocketAddr, token: String) -> Result<Self> {
        ensure!(
            address.ip().is_loopback(),
            "{}",
            crate::i18n::format("host-debug-0003", &[])
        );
        validate_token(&token)?;
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (sender, receiver) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("editor-debug".into())
            .spawn(move || {
                let mut transport = Transport {
                    sender,
                    token,
                    stop: stopping,
                    highest: 0,
                    recent: BTreeMap::new(),
                    order: VecDeque::new(),
                };
                while !transport.stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = transport.connection(stream);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            address,
            receiver,
            stop,
            worker: Some(worker),
        })
    }
    pub fn try_next(&self) -> Option<Job> {
        self.receiver.try_recv().ok()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
