//! Deterministic, low-volume demo sounds. Not a character voice recording.
use std::{f32::consts::TAU, io::Write, path::Path};

fn write_wav(path: &Path, seconds: f32, sample: impl Fn(f32) -> f32) -> std::io::Result<()> {
    let rate = 22_050_u32;
    let count = (seconds * rate as f32) as u32;
    let bytes = count * 2;
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for index in 0..count {
        let t = index as f32 / rate as f32;
        let value = (sample(t).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/audio");
    std::fs::create_dir_all(&root)?;
    write_wav(&root.join("room.wav"), 4.0, |t| {
        let envelope = (t * 2.0).min(1.0) * ((4.0 - t) * 2.0).min(1.0);
        envelope
            * 0.07
            * ((TAU * 220.0 * t).sin()
                + 0.5 * (TAU * 330.0 * t).sin()
                + 0.3 * (TAU * 440.0 * t).sin())
    })?;
    write_wav(&root.join("knock.wav"), 0.7, |t| {
        [0.0, 0.22, 0.44]
            .iter()
            .map(|start| {
                let d = t - start;
                if (0.0..0.16).contains(&d) {
                    0.35 * (-35.0 * d).exp() * (TAU * 180.0 * d).sin()
                } else {
                    0.0
                }
            })
            .sum()
    })?;
    write_wav(&root.join("lin.wav"), 0.65, |t| {
        let frequency = if t < 0.3 { 523.25 } else { 659.25 };
        let envelope = (t * 25.0).min(1.0) * ((0.65 - t) * 15.0).min(1.0);
        0.1 * envelope * (TAU * frequency * t).sin()
    })?;
    println!(
        "Generated demo BGM, knock and voice-channel cue in {}",
        root.display()
    );
    Ok(())
}
