//! Writes each of Flow's sounds (`flow_core::sound`) to a WAV file, made and
//! mixed as Flow plays them at a Volume of 60%, to listen to them away from
//! Flow: one file a cue, centered, the old machines' own (`hum.wav`,
//! `margin.wav`, `return.wav`, `seek.wav`, `punch.wav`, `print.wav`,
//! `fan.wav`, `fan-air.wav`, `tick.wav`, `beep.wav`, `startup.wav`) too,
//! `notes.wav`, a DECPS scale, and a minute of a tube's hum
//! (`hum-minute.wav`) and of the PC's fan with it (`fan-minute.wav`), laid
//! on every `HUM_EVERY` at the gains Flow plays them at, the fan's other
//! air on every `FAN_AIR_EVERY`, to hear them drift.
//!
//!   cargo run --release --example sounds -- <folder>

use flow_core::sound::{note_hz, Cue, Mixer, Voice, FAN_AIR_EVERY, HUM_EVERY, NAMES};

const RATE: u32 = 48_000;

fn main() -> anyhow::Result<()> {
    let Some(dir) = std::env::args().nth(1) else {
        anyhow::bail!("usage: sounds <folder>");
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    let machines = [("hum", Cue::Hum), ("margin", Cue::Margin), ("return", Cue::Return), ("seek", Cue::Seek), ("punch", Cue::Punch), ("print", Cue::Print), ("fan", Cue::Fan), ("fan-air", Cue::FanAir), ("tick", Cue::Tick), ("beep", Cue::Beep), ("startup", Cue::Startup), ("rainfall", Cue::Rainfall), ("step", Cue::Step)];
    for (name, cue) in NAMES.into_iter().chain(machines) {
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, &mix(vec![Voice::new(cue, 0.0, 1.0, 0.5, 0.0)], cue.length()))?;
        println!("{}", path.display());
    }
    // C major up and down, an eighth of a second a note.
    let scale = [1u8, 3, 5, 6, 8, 10, 12, 13, 12, 10, 8, 6, 5, 3, 1];
    let voices = scale
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| Some(Voice::new(Cue::Note { hz: note_hz(n)?, secs: 0.125 }, 0.0, 1.0, 0.0, i as f32 * 0.125)))
        .collect();
    let path = dir.join("notes.wav");
    write_wav(&path, &mix(voices, scale.len() as f32 * 0.125 + 0.1))?;
    println!("{}", path.display());
    // A minute of each, as `App::hear_machines` lays them: each cue on
    // its spacing, at its gain, while the minute lasts, each voice with
    // noise of its own, as Flow's `Sound` gives it.
    let laid = |cues: &[(Cue, std::time::Duration, f32)]| -> Vec<Voice> {
        let each = |(k, &(cue, every, gain)): (usize, &(Cue, std::time::Duration, f32))| {
            (0u32..).map(move |i| (i, i as f32 * every.as_secs_f32())).take_while(|&(_, at)| at < 60.0).map(move |(i, at)| Voice::new(cue, 0.0, gain, 0.0, at).seeded((k as u32 * 1000 + i).wrapping_mul(2_654_435_761)))
        };
        cues.iter().enumerate().flat_map(each).collect()
    };
    let hum = (Cue::Hum, HUM_EVERY, 0.12);
    for (name, cues) in [("hum-minute", vec![hum]), ("fan-minute", vec![hum, (Cue::Fan, HUM_EVERY, 0.5), (Cue::FanAir, FAN_AIR_EVERY, 0.5)])] {
        let path = dir.join(format!("{name}.wav"));
        let longest = cues.iter().map(|(cue, ..)| cue.length()).fold(0.0, f32::max);
        write_wav(&path, &mix(laid(&cues), 60.0 + longest))?;
        println!("{}", path.display());
    }
    Ok(())
}

/// `voices` mixed for `secs`, left and right.
fn mix(voices: Vec<Voice>, secs: f32) -> Vec<f32> {
    let mut mixer = Mixer::new(0.6);
    for v in voices {
        mixer.add(v);
    }
    let mut out = vec![0.0; (secs * RATE as f32) as usize * 2];
    mixer.fill(&mut out, 2, RATE as f32);
    out
}

/// 16-bit stereo PCM.
fn write_wav(path: &std::path::Path, samples: &[f32]) -> anyhow::Result<()> {
    let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(RATE.to_le_bytes());
    wav.extend((RATE * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(path, wav)?;
    Ok(())
}
