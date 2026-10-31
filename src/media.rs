//! Bounded video streaming. Decode and upload happen outside drawing and story logic.
use crate::{
    assets::AssetCache,
    content::ContentSource,
    model::{AssetKind, Story},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use rodio::buffer::SamplesBuffer;
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, mpsc},
    thread,
};
#[cfg(windows)]
mod windows_mf;

pub struct VideoAsset {
    pub file: tempfile::NamedTempFile,
    pub size: [usize; 2],
    pub duration: f64,
    pub audio: Option<SamplesBuffer>,
    pub first: egui::ColorImage,
}
#[derive(Default)]
pub struct PreparedMedia {
    pub videos: BTreeMap<String, Arc<VideoAsset>>,
}
impl PreparedMedia {
    pub fn load(story: &Story, source: &dyn ContentSource) -> Result<Self> {
        let mut videos = BTreeMap::new();
        let mut total = 0usize;
        for (id, a) in &story.assets {
            if a.kind != AssetKind::Video {
                continue;
            }
            ensure!(
                videos.len() < 16,
                "{}",
                crate::i18n::format("host-media-0001", &[])
            );
            let mut file = tempfile::Builder::new().suffix(".mp4").tempfile()?;
            source
                .copy_to(&a.path, file.as_file_mut())
                .with_context(|| {
                    crate::i18n::format("host-media-0002", &[("arg0", id.to_string())])
                })?;
            file.flush()?;
            #[cfg(windows)]
            let asset = windows_mf::prepare(file).with_context(|| {
                crate::i18n::format(
                    "host-media-0003",
                    &[
                        ("arg0", id.to_string()),
                        ("arg1", format!("{}", a.path.display())),
                    ],
                )
            })?;
            #[cfg(not(windows))]
            let asset: VideoAsset = {
                let _ = file;
                anyhow::bail!("{}", crate::i18n::format("host-media-0004", &[]))
            };
            if let Some(audio) = &asset.audio {
                total += audio.size_hint().0 * 4;
            }
            ensure!(
                total <= 256 * 1024 * 1024,
                "{}",
                crate::i18n::format("host-media-0005", &[])
            );
            videos.insert(id.clone(), Arc::new(asset));
        }
        Ok(Self { videos })
    }
}
pub struct DecodedFrame {
    pub image: egui::ColorImage,
    pub timestamp: f64,
    pub ended: bool,
}
struct Worker {
    requests: Option<mpsc::SyncSender<f64>>,
    frames: mpsc::Receiver<Result<DecodedFrame>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    fn new(asset: Arc<VideoAsset>) -> Self {
        let (tx, requests) = mpsc::sync_channel(1);
        let (frames, rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            #[cfg(windows)]
            let decoder = windows_mf::Decoder::new(asset.file.path());
            #[cfg(not(windows))]
            let decoder: Result<()> = Err(anyhow::anyhow!(
                "{}",
                crate::i18n::format("host-media-0006", &[])
            ));
            let mut decoder = match decoder {
                Ok(d) => d,
                Err(e) => {
                    let _ = frames.try_send(Err(e));
                    return;
                }
            };
            while let Ok(position) = requests.recv() {
                #[cfg(windows)]
                let result = decoder.frame(position);
                #[cfg(not(windows))]
                let result: Result<DecodedFrame> = Err(anyhow::anyhow!(
                    "{}",
                    crate::i18n::format("host-media-0007", &[])
                ));
                let failed = result.is_err();
                // The UI retains the last image; never block the producer on a full output queue.
                let _ = frames.try_send(result);
                if failed {
                    break;
                }
            }
        });
        Self {
            requests: Some(tx),
            frames: rx,
            thread: Some(thread),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoCue {
    pub asset: String,
    pub skippable: bool,
}
#[derive(Default)]
pub struct VideoPlayer {
    worker: Option<Worker>,
    pub cue: Option<VideoCue>,
    pub position: f64,
    pub duration: f64,
    pub ended: bool,
    pub failed: bool,
    pub first: Option<egui::ColorImage>,
    pub texture_ready: bool,
}
impl VideoPlayer {
    pub fn prepare(cue: Option<&VideoCue>, media: &PreparedMedia) -> Result<Self> {
        let Some(cue) = cue else {
            return Ok(Self::default());
        };
        let asset = media
            .videos
            .get(&cue.asset)
            .context(crate::i18n::text("host-media-0008"))?
            .clone();
        Ok(Self {
            worker: Some(Worker::new(asset.clone())),
            cue: Some(cue.clone()),
            duration: asset.duration,
            first: Some(asset.first.clone()),
            ..Default::default()
        })
    }
    pub fn playing(&self) -> bool {
        self.cue.is_some() && !self.ended
    }
    pub fn skip(&mut self) -> bool {
        if self.cue.as_ref().is_some_and(|c| c.skippable) {
            self.ended = true;
            self.worker = None;
            true
        } else {
            false
        }
    }
    pub fn tick(
        &mut self,
        seconds: f32,
        ctx: &egui::Context,
        assets: &mut AssetCache,
    ) -> Result<bool> {
        self.tick_named(seconds, ctx, assets, "$video")
    }
    pub fn tick_named(
        &mut self,
        seconds: f32,
        ctx: &egui::Context,
        assets: &mut AssetCache,
        texture: &str,
    ) -> Result<bool> {
        if let Some(first) = self.first.take() {
            assets.update_stream(ctx, texture, first);
            self.texture_ready = true;
        }
        if !self.playing() {
            return Ok(self.ended);
        }
        if self.failed {
            return Ok(false);
        }
        let worker = self.worker.as_ref().expect("playing worker");
        while let Ok(result) = worker.frames.try_recv() {
            match result {
                Ok(frame) => {
                    assets.update_stream(ctx, texture, frame.image);
                    self.texture_ready = true;
                    if frame.ended {
                        self.ended = true;
                    }
                }
                Err(e) => {
                    self.failed = true;
                    return Err(e);
                }
            }
        }
        self.position = (self.position + f64::from(seconds.max(0.0))).min(self.duration);
        if !self.failed && seconds > 0.0 {
            let _ = worker
                .requests
                .as_ref()
                .expect("worker sender")
                .try_send(self.position);
        }
        if self.position >= self.duration {
            self.ended = true;
        }
        Ok(self.ended)
    }
}
