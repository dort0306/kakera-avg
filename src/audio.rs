use crate::{
    model::{AssetKind, AudioCue, Story},
    settings::Settings,
};
use anyhow::{Context, Result, ensure};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source, buffer::SamplesBuffer};
use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::mpsc::{self, Receiver},
};

const MAX_CLIP_BYTES: usize = 128 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Default)]
pub struct PreparedAudio {
    clips: BTreeMap<String, SamplesBuffer>,
}

impl PreparedAudio {
    pub fn insert_video(&mut self, id: &str, audio: SamplesBuffer) {
        self.clips.insert(format!("$video:{id}"), audio);
    }
    pub fn load_from(story: &Story, source: &dyn crate::content::ContentSource) -> Result<Self> {
        Self::load_bytes(story, &|path| source.read(path))
    }
    fn load_bytes(
        story: &Story,
        read: &dyn Fn(&std::path::Path) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let mut clips = BTreeMap::new();
        let mut total_bytes = 0;
        for (id, asset) in &story.assets {
            if asset.kind != AssetKind::Audio {
                continue;
            }
            let clip = (|| -> Result<SamplesBuffer> {
                let bytes = read(&asset.path)?;
                ensure!(
                    bytes.len() <= MAX_CLIP_BYTES,
                    "{}",
                    crate::i18n::format("host-audio-0001", &[])
                );
                let decoder = Decoder::try_from(Cursor::new(bytes))
                    .context(crate::i18n::text("host-audio-0012"))?;
                let channels = decoder.channels();
                let sample_rate = decoder.sample_rate();
                ensure!(
                    (1..=2).contains(&channels),
                    "{}",
                    crate::i18n::format("host-audio-0002", &[])
                );
                ensure!(
                    (8000..=96000).contains(&sample_rate),
                    "{}",
                    crate::i18n::format("host-audio-0003", &[])
                );
                let max_samples = (sample_rate as usize * channels as usize * 300)
                    .min(MAX_CLIP_BYTES / 4)
                    .min((MAX_CACHE_BYTES - total_bytes) / 4);
                let samples: Vec<f32> = decoder.take(max_samples + 1).collect();
                ensure!(
                    !samples.is_empty(),
                    "{}",
                    crate::i18n::format("host-audio-0004", &[])
                );
                ensure!(
                    samples.len() <= max_samples,
                    "{}",
                    crate::i18n::format("host-audio-0005", &[])
                );
                ensure!(
                    samples.iter().all(|sample| sample.is_finite()),
                    "{}",
                    crate::i18n::format("host-audio-0006", &[])
                );
                ensure!(
                    samples.len().is_multiple_of(channels as usize),
                    "{}",
                    crate::i18n::format("host-audio-0007", &[])
                );
                total_bytes += samples.len() * 4;
                Ok(SamplesBuffer::new(channels, sample_rate, samples))
            })()
            .with_context(|| {
                crate::i18n::format(
                    "host-audio-0008",
                    &[
                        ("arg0", id.to_string()),
                        ("arg1", format!("{}", asset.path.display())),
                    ],
                )
            })?;
            clips.insert(id.clone(), clip);
        }
        Ok(Self { clips })
    }
    pub fn len(&self) -> usize {
        self.clips.len()
    }
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
    pub fn clip(&self, id: &str) -> Option<&SamplesBuffer> {
        self.clips.get(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Bgm,
    Voice,
    Se,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioCommand {
    Stop(Channel),
    Play {
        channel: Channel,
        id: String,
        looping: bool,
    },
}

/// Pure scene-entry planning, independent of the audio device and redraw loop.
#[derive(Default)]
pub struct AudioPlan {
    bgm: Option<String>,
}

impl AudioPlan {
    pub fn enter(&mut self, cue: &AudioCue, force: bool) -> Vec<AudioCommand> {
        let mut commands = Vec::new();
        if force || self.bgm != cue.bgm {
            commands.push(AudioCommand::Stop(Channel::Bgm));
            if let Some(id) = &cue.bgm {
                commands.push(AudioCommand::Play {
                    channel: Channel::Bgm,
                    id: id.clone(),
                    looping: true,
                });
            }
        }
        self.bgm = cue.bgm.clone();
        for (channel, id) in [(Channel::Voice, &cue.voice), (Channel::Se, &cue.se)] {
            commands.push(AudioCommand::Stop(channel));
            if let Some(id) = id {
                commands.push(AudioCommand::Play {
                    channel,
                    id: id.clone(),
                    looping: false,
                });
            }
        }
        commands
    }
}

struct Device {
    ui: Vec<(String, Sink)>,
    video: Option<Sink>,
    // Keep sinks alive, and drop them before the output stream.
    bgm: Option<Sink>,
    voice: Option<Sink>,
    se: Option<Sink>,
    stream: OutputStream,
    errors: Receiver<String>,
}

pub struct AudioEngine {
    video_active: bool,
    prepared: PreparedAudio,
    plan: AudioPlan,
    device: Option<Device>,
    status: String,
}

impl AudioEngine {
    pub fn play_ui(&mut self, id: &str) {
        if let Some(device) = &mut self.device
            && let Some(clip) = self.prepared.clip(id)
        {
            if device.ui.len() >= 4 {
                let (_, old) = device.ui.remove(0);
                old.stop();
            }
            let sink = Sink::connect_new(device.stream.mixer());
            sink.pause();
            sink.append(clip.clone());
            device.ui.push((id.into(), sink));
        }
    }
    pub fn configure_ui(
        &mut self,
        settings: &Settings,
        focused: bool,
        allowed: impl Fn(&str) -> bool,
    ) {
        if let Some(device) = &mut self.device {
            device.ui.retain(|(id, sink)| {
                if !allowed(id) {
                    sink.stop();
                    return false;
                }
                !sink.empty()
            });
            for (_, sink) in &device.ui {
                sink.set_volume(if settings.muted {
                    0.
                } else {
                    settings.se_volume
                });
                if focused { sink.play() } else { sink.pause() }
            }
        }
    }

    pub fn video(&mut self, id: Option<&str>) {
        self.video_active = id.is_some();
        if let Some(device) = &mut self.device {
            if let Some(old) = device.video.take() {
                old.stop();
            }
            if let Some(clip) = id.and_then(|id| self.prepared.clip(&format!("$video:{id}"))) {
                let sink = Sink::connect_new(device.stream.mixer());
                sink.pause();
                sink.append(clip.clone());
                device.video = Some(sink);
            }
        }
    }
    pub fn voice_finished(&self) -> bool {
        self.device
            .as_ref()
            .and_then(|d| d.voice.as_ref())
            .is_none_or(Sink::empty)
    }
    pub fn new(prepared: PreparedAudio, disabled: bool) -> (Self, Option<String>) {
        let mut warning = None;
        let (device, status) = if disabled || prepared.is_empty() {
            (
                None,
                if disabled {
                    crate::i18n::text("host-audio-0013")
                } else {
                    crate::i18n::text("host-audio-0014")
                }
                .to_owned(),
            )
        } else {
            let (sender, errors) = mpsc::sync_channel(1);
            let result = OutputStreamBuilder::from_default_device().and_then(|builder| {
                builder
                    .with_error_callback(move |error| {
                        let _ = sender.try_send(format!("{error}"));
                    })
                    .open_stream_or_fallback()
            });
            match result {
                Ok(mut stream) => {
                    stream.log_on_drop(false);
                    eprintln!(
                        "{}",
                        crate::i18n::format(
                            "host-audio-0009",
                            &[("arg0", format!("{:?}", stream.config()))]
                        )
                    );
                    (
                        Some(Device {
                            ui: Vec::new(),
                            video: None,
                            bgm: None,
                            voice: None,
                            se: None,
                            stream,
                            errors,
                        }),
                        crate::i18n::text("host-audio-0015").to_owned(),
                    )
                }
                Err(error) => {
                    warning = Some(crate::i18n::format(
                        "host-audio-0010",
                        &[("arg0", error.to_string())],
                    ));
                    (None, crate::i18n::text("host-audio-0016").to_owned())
                }
            }
        };
        (
            Self {
                video_active: false,
                prepared,
                plan: AudioPlan::default(),
                device,
                status,
            },
            warning,
        )
    }

    pub fn enter(&mut self, cue: &AudioCue, force: bool) {
        let commands = self.plan.enter(cue, force);
        let Some(device) = self.device.as_mut() else {
            return;
        };
        for command in commands {
            let channel = match &command {
                AudioCommand::Stop(channel) | AudioCommand::Play { channel, .. } => channel,
            };
            let target = match channel {
                Channel::Bgm => &mut device.bgm,
                Channel::Voice => &mut device.voice,
                Channel::Se => &mut device.se,
            };
            match command {
                AudioCommand::Stop(_) => {
                    if let Some(sink) = target.take() {
                        sink.stop();
                    }
                }
                AudioCommand::Play { id, looping, .. } => {
                    if let Some(clip) = self.prepared.clip(&id) {
                        let sink = Sink::connect_new(device.stream.mixer());
                        // App applies current volume/pause before returning control to rendering.
                        sink.pause();
                        if looping {
                            sink.append(clip.clone().repeat_infinite());
                        } else {
                            sink.append(clip.clone());
                        }
                        *target = Some(sink);
                    }
                }
            }
        }
    }

    pub fn stop_voice(&mut self) {
        if let Some(device) = self.device.as_mut()
            && let Some(voice) = device.voice.take()
        {
            voice.stop();
        }
    }
    /// Hot reload swaps prepared clips without replaying any active channel.
    pub fn replace_prepared(&mut self, prepared: PreparedAudio) {
        self.prepared = prepared;
    }

    pub fn configure_ui_video(&self, settings: &Settings, paused: bool) {
        if let Some(sink) = self.device.as_ref().and_then(|d| d.video.as_ref()) {
            sink.set_volume(if settings.muted {
                0.0
            } else {
                settings.video_volume
            });
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }

    pub fn configure(&self, settings: &Settings, overlay: bool, focused: bool) {
        let Some(device) = &self.device else {
            return;
        };
        for (sink, volume, pause) in [
            (
                &device.bgm,
                settings.bgm_volume,
                !focused || self.video_active,
            ),
            (
                &device.voice,
                settings.voice_volume,
                !focused || overlay || self.video_active,
            ),
            (
                &device.se,
                settings.se_volume,
                !focused || overlay || self.video_active,
            ),
            (&device.video, settings.video_volume, !focused || overlay),
        ] {
            if let Some(sink) = sink {
                sink.set_volume(if settings.muted { 0.0 } else { volume });
                if pause {
                    sink.pause();
                } else {
                    sink.play();
                }
            }
        }
    }

    pub fn poll_error(&mut self) -> Option<String> {
        let error = self.device.as_ref()?.errors.try_recv().ok()?;
        self.device = None;
        self.status = crate::i18n::text("host-audio-0017").into();
        Some(crate::i18n::format(
            "host-audio-0011",
            &[("arg0", error.to_string())],
        ))
    }

    pub fn status(&self) -> String {
        let mut status = self.status.clone();
        if let Some(device) = &self.device {
            for (label, sink) in [
                ("BGM", &device.bgm),
                ("VOICE", &device.voice),
                ("SE", &device.se),
            ] {
                let state = match sink {
                    Some(sink) if !sink.empty() => {
                        if sink.is_paused() {
                            crate::i18n::text("host-audio-0018")
                        } else {
                            crate::i18n::text("host-audio-0019")
                        }
                    }
                    _ => crate::i18n::text("host-audio-0020"),
                };
                status.push_str(&format!("\n{label}: {state}"));
            }
        }
        status
    }
}
