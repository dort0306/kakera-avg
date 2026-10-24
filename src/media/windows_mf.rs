//! Media Foundation owns decoder state on the calling thread. No COM object crosses threads.
use super::{DecodedFrame, VideoAsset};
use anyhow::{Context, Result, ensure};
use eframe::egui::ColorImage;
use windows::{
    Win32::{Media::MediaFoundation::*, System::Com::*},
    core::{Interface, PCWSTR},
};
struct ContextGuard;
impl ContextGuard {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(e.into());
            }
        }
        Ok(Self)
    }
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
fn reader(path: &std::path::Path) -> Result<IMFSourceReader> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.context("MF attributes")?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let text: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        Ok(MFCreateSourceReaderFromURL(PCWSTR(text.as_ptr()), &attrs)?)
    }
}
pub struct Decoder {
    reader: IMFSourceReader,
    size: [usize; 2],
    stride: i32,
    last: ColorImage,
    last_time: f64,
    ended: bool,
    _guard: ContextGuard,
}
impl Decoder {
    pub fn new(path: &std::path::Path) -> Result<Self> {
        unsafe {
            let guard = ContextGuard::new()?;
            let reader = reader(path)?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            reader.SetCurrentMediaType(stream, None, &mt)?;
            reader.SetStreamSelection(stream, true)?;
            let current = reader.GetCurrentMediaType(stream)?;
            let packed = current.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let size = [(packed >> 32) as usize, (packed & 0xffff_ffff) as usize];
            ensure!(
                size[0] > 0 && size[1] > 0 && size[0] <= 1920 && size[1] <= 1080,
                "视频尺寸须在 1920×1080 内"
            );
            let rate = current.GetUINT64(&MF_MT_FRAME_RATE)?;
            let fps = (rate >> 32) as f64 / (rate & 0xffff_ffff) as f64;
            ensure!(
                fps.is_finite() && fps > 0.0 && fps <= 60.1,
                "视频帧率须在 0–60fps"
            );
            let stride = current
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map(|s| s as i32)
                .unwrap_or((size[0] * 4) as i32);
            Ok(Self {
                reader,
                size,
                stride,
                last: ColorImage::filled(size, eframe::egui::Color32::BLACK),
                last_time: -1.0,
                ended: false,
                _guard: guard,
            })
        }
    }
    pub fn duration(&self) -> Result<f64> {
        unsafe {
            let value = self
                .reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)?;
            let duration = u64::try_from(&value)? as f64 / 10_000_000.0;
            ensure!(duration > 0.0 && duration <= 300.0, "视频时长须在 0–300 秒");
            Ok(duration)
        }
    }
    pub fn frame(&mut self, target: f64) -> Result<DecodedFrame> {
        unsafe {
            // A malformed source cannot spin indefinitely without producing samples.
            let mut reads = 0;
            while !self.ended && self.last_time < target {
                reads += 1;
                ensure!(reads <= 18001, "视频解码读取预算耗尽");
                let mut flags = 0;
                let mut time = 0;
                let mut sample = None;
                self.reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )?;
                self.ended = flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0;
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                    anyhow::bail!("播放中视频格式改变，v0.6 不支持");
                }
                if let Some(sample) = sample {
                    let buffer = sample.ConvertToContiguousBuffer()?;
                    let mut rgba = vec![0u8; self.size[0] * self.size[1] * 4];
                    if let Ok(two) = buffer.cast::<IMF2DBuffer>() {
                        let mut top = std::ptr::null_mut();
                        let mut pitch = 0;
                        two.Lock2D(&mut top, &mut pitch)?;
                        let result = copy_rgb(top, pitch, self.size, &mut rgba);
                        let unlocked = two.Unlock2D();
                        result?;
                        unlocked?;
                    } else {
                        let mut bytes = std::ptr::null_mut();
                        let mut len = 0;
                        buffer.Lock(&mut bytes, None, Some(&mut len))?;
                        let pitch = self.stride;
                        let valid = !bytes.is_null()
                            && pitch.unsigned_abs() as usize >= self.size[0] * 4
                            && len as usize >= pitch.unsigned_abs() as usize * self.size[1];
                        let result = if valid {
                            let top = if pitch < 0 {
                                bytes.add(pitch.unsigned_abs() as usize * (self.size[1] - 1))
                            } else {
                                bytes
                            };
                            copy_rgb(top, pitch, self.size, &mut rgba)
                        } else {
                            Err(anyhow::anyhow!("视频缓冲尺寸无效"))
                        };
                        let unlocked = buffer.Unlock();
                        result?;
                        unlocked?;
                    }
                    self.last = ColorImage::from_rgba_unmultiplied(self.size, &rgba);
                    self.last_time = time as f64 / 10_000_000.0;
                }
            }
            Ok(DecodedFrame {
                image: self.last.clone(),
                timestamp: self.last_time,
                ended: self.ended,
            })
        }
    }
}
unsafe fn copy_rgb(top: *const u8, pitch: i32, size: [usize; 2], out: &mut [u8]) -> Result<()> {
    ensure!(
        !top.is_null() && pitch.unsigned_abs() as usize >= size[0] * 4,
        "无效视频行缓冲"
    );
    for y in 0..size[1] {
        // SAFETY: MF's locked buffer provides height rows at its documented signed pitch.
        let row = unsafe {
            std::slice::from_raw_parts(top.offset(y as isize * pitch as isize), size[0] * 4)
        };
        for x in 0..size[0] {
            let i = x * 4;
            let o = (y * size[0] + x) * 4;
            out[o..o + 4].copy_from_slice(&[row[i + 2], row[i + 1], row[i], 255]);
        }
    }
    Ok(())
}
fn audio(path: &std::path::Path) -> Result<Option<rodio::buffer::SamplesBuffer>> {
    unsafe {
        let reader = reader(path)?;
        let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        if let Err(e) = reader.GetNativeMediaType(stream, 0) {
            if e.code() == MF_E_INVALIDSTREAMNUMBER {
                return Ok(None);
            }
            return Err(e.into());
        }
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
        mt.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        mt.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48000)?;
        reader.SetCurrentMediaType(stream, None, &mt)?;
        reader.SetStreamSelection(stream, true)?;
        let mut pcm = Vec::new();
        for _ in 0..100000 {
            let mut flags = 0;
            let mut sample = None;
            reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            if let Some(sample) = sample {
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut data = std::ptr::null_mut();
                let mut len = 0;
                buffer.Lock(&mut data, None, Some(&mut len))?;
                let result = (|| -> Result<()> {
                    ensure!(
                        len % 8 == 0 && pcm.len() + len as usize / 4 <= 48000 * 2 * 300,
                        "视频音频超过 300 秒或缓冲无效"
                    );
                    ensure!(len == 0 || !data.is_null(), "视频音频指针为空");
                    if len > 0 {
                        for sample in std::slice::from_raw_parts(data, len as usize)
                            .as_chunks::<4>()
                            .0
                        {
                            let v = f32::from_le_bytes(*sample);
                            ensure!(v.is_finite(), "无效音频采样");
                            pcm.push(v);
                        }
                    }
                    Ok(())
                })();
                let unlocked = buffer.Unlock();
                result?;
                unlocked?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(
                    (!pcm.is_empty()).then(|| rodio::buffer::SamplesBuffer::new(2, 48000, pcm))
                );
            }
        }
        anyhow::bail!("视频音频解码预算耗尽")
    }
}
pub fn prepare(file: tempfile::NamedTempFile) -> Result<VideoAsset> {
    let mut decoder = Decoder::new(file.path())?;
    let duration = decoder.duration()?;
    let first = decoder.frame(0.0)?.image;
    let audio = audio(file.path())?;
    let size = decoder.size;
    drop(decoder);
    Ok(VideoAsset {
        file,
        size,
        duration,
        audio,
        first,
    })
}
