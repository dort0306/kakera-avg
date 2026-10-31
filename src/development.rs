//! Native events, debouncing and bounded candidate preparation outside drawing.
use crate::{
    package::StagedProject,
    runtime::{Bundle, LoadOptions},
};
use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
pub struct Candidate {
    pub revision: u64,
    pub result: Result<Bundle>,
}
pub fn compatible_session(
    current: &crate::model::Story,
    session: &crate::model::Session,
    replacement: &crate::model::Story,
    logic: &dyn crate::model::Logic,
) -> Result<crate::model::Session> {
    anyhow::ensure!(
        current.id == replacement.id && current.content_version == replacement.content_version,
        "{}",
        crate::i18n::format("host-development-0001", &[])
    );
    replacement
        .validate_session_with(session, logic)
        .map_err(anyhow::Error::msg)?;
    logic
        .options(replacement, session)
        .map_err(anyhow::Error::msg)?;
    Ok(session.clone())
}
pub struct ProjectMonitor {
    root: PathBuf,
    options: LoadOptions,
    revision: Arc<AtomicU64>,
    _watcher: notify::RecommendedWatcher,
    seen: u64,
    started: u64,
    changed: Instant,
    building: Option<Receiver<Candidate>>,
    pub status: String,
    pub error: Option<String>,
    pub pending: Option<Bundle>,
}
impl ProjectMonitor {
    pub fn new(root: &Path, options: LoadOptions) -> Result<Self> {
        let root = root.canonicalize()?;
        let revision = Arc::new(AtomicU64::new(0));
        let counter = revision.clone();
        let watch_root = root.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let changed = match event {
                    Ok(event) => {
                        !matches!(event.kind, EventKind::Access(_))
                            && event.paths.iter().any(|path| {
                                let Ok(path) = path.strip_prefix(&watch_root) else {
                                    return false;
                                };
                                !path.components().any(|c| {
                                    crate::package::excluded(&c.as_os_str().to_string_lossy(), true)
                                })
                            })
                    }
                    Err(_) => true,
                };
                if changed {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        Ok(Self {
            root,
            options,
            revision,
            _watcher: watcher,
            seen: 0,
            started: 0,
            changed: Instant::now(),
            building: None,
            status: crate::i18n::text("host-development-0003").into(),
            error: None,
            pending: None,
        })
    }
    pub fn poll(&mut self) -> Option<Result<Bundle>> {
        let revision = self.revision.load(Ordering::SeqCst);
        if revision != self.seen {
            self.seen = revision;
            self.changed = Instant::now();
            self.pending = None;
            self.status = crate::i18n::text("host-development-0004").into();
        }
        if let Some(receiver) = &self.building {
            match receiver.try_recv() {
                Ok(candidate) => {
                    self.building = None;
                    if candidate.revision == self.revision.load(Ordering::SeqCst) {
                        return Some(candidate.result);
                    }
                    self.status = crate::i18n::text("host-development-0005").into();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.building = None;
                    return Some(Err(anyhow::anyhow!(
                        "{}",
                        crate::i18n::format("host-development-0002", &[])
                    )));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.building.is_none()
            && self.seen != self.started
            && self.changed.elapsed() >= Duration::from_millis(200)
        {
            self.started = self.seen;
            let revision = self.seen;
            let root = self.root.clone();
            let options = self.options.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            self.building = Some(rx);
            self.status = crate::i18n::text("host-development-0006").into();
            std::thread::spawn(move || {
                let result = (|| {
                    let snapshot = StagedProject::capture(&root)?;
                    let bundle = Bundle::load(&snapshot, &options)?;
                    snapshot.ensure_unchanged()?;
                    Ok(bundle)
                })();
                let _ = tx.send(Candidate { revision, result });
            });
        }
        None
    }
    pub fn request(&mut self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
    }
}
