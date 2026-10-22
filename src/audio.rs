use crate::{
    model::{AssetKind, AudioCue, Story},
    settings::Settings,
};
use anyhow::{Context, Result, ensure};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source, buffer::SamplesBuffer};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    sync::mpsc::{self, Receiver},
};

const MAX_CLIP_BYTES: usize = 128 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Default)]
pub struct PreparedAudio {
    clips: BTreeMap<String, SamplesBuffer>,
}

impl PreparedAudio {
    pub fn load(story: &Story) -> Result<Self> {
        let mut clips = BTreeMap::new();
        let mut total_bytes = 0;
        for (id, asset) in &story.assets {
            if asset.kind != AssetKind::Audio {
                continue;
            }
            let clip = (|| -> Result<SamplesBuffer> {
                let mut bytes = Vec::new();
                std::fs::File::open(&asset.path)?
                    .take(MAX_CLIP_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= MAX_CLIP_BYTES, "音频文件超过 128MiB");
                let decoder =
                    Decoder::try_from(Cursor::new(bytes)).context("无法解码 WAV/Ogg 音频")?;
                let channels = decoder.channels();
                let sample_rate = decoder.sample_rate();
                ensure!((1..=2).contains(&channels), "音频必须为单声道或双声道");
                ensure!(
                    (8000..=96000).contains(&sample_rate),
                    "音频采样率必须在 8000–96000 Hz 之间"
                );
                let max_samples = (sample_rate as usize * channels as usize * 300)
                    .min(MAX_CLIP_BYTES / 4)
                    .min((MAX_CACHE_BYTES - total_bytes) / 4);
                let samples: Vec<f32> = decoder.take(max_samples + 1).collect();
                ensure!(!samples.is_empty(), "音频没有可播放采样");
                ensure!(
                    samples.len() <= max_samples,
                    "音频超过 5 分钟、单资源 128MiB 或缓存总量 256MiB 上限"
                );
                ensure!(
                    samples.iter().all(|sample| sample.is_finite()),
                    "音频包含非法采样"
                );
                ensure!(
                    samples.len().is_multiple_of(channels as usize),
                    "音频采样帧不完整"
                );
                total_bytes += samples.len() * 4;
                Ok(SamplesBuffer::new(channels, sample_rate, samples))
            })()
            .with_context(|| format!("音频资源 {id}：{}", asset.path.display()))?;
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
    // Keep sinks alive, and drop them before the output stream.
    bgm: Option<Sink>,
    voice: Option<Sink>,
    se: Option<Sink>,
    stream: OutputStream,
    errors: Receiver<String>,
}

pub struct AudioEngine {
    prepared: PreparedAudio,
    plan: AudioPlan,
    device: Option<Device>,
    status: String,
}

impl AudioEngine {
    pub fn new(prepared: PreparedAudio, disabled: bool) -> (Self, Option<String>) {
        let mut warning = None;
        let (device, status) = if disabled || prepared.is_empty() {
            (
                None,
                if disabled {
                    "音频输出已禁用"
                } else {
                    "故事未配置音频"
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
                    eprintln!("音频设备输出流已建立：{:?}", stream.config());
                    (
                        Some(Device {
                            bgm: None,
                            voice: None,
                            se: None,
                            stream,
                            errors,
                        }),
                        "音频设备输出正常".to_owned(),
                    )
                }
                Err(error) => {
                    warning = Some(format!("音频设备不可用，已降级为静音：{error}"));
                    (None, "音频设备不可用".to_owned())
                }
            }
        };
        (
            Self {
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

    pub fn configure(&self, settings: &Settings, overlay: bool, focused: bool) {
        let Some(device) = &self.device else {
            return;
        };
        for (sink, volume, pause) in [
            (&device.bgm, settings.bgm_volume, !focused),
            (&device.voice, settings.voice_volume, !focused || overlay),
            (&device.se, settings.se_volume, !focused || overlay),
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
        self.status = "音频设备不可用".into();
        Some(format!("音频输出中断，已降级为静音：{error}"))
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
                            "暂停"
                        } else {
                            "正在播放"
                        }
                    }
                    _ => "停止",
                };
                status.push_str(&format!("\n{label}: {state}"));
            }
        }
        status
    }
}
