//! Synthesised sounds (no audio files, no audio crates): bell-like tones rendered into
//! in-memory WAVs and played with `PlaySoundW(SND_MEMORY | SND_ASYNC)` on Windows.

use std::f32::consts::TAU;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub enum Sound {
    /// Work segment ended, break starts: soft descending chime.
    BreakStart,
    /// Break ended / waiting reminder: loud alarm-clock ring.
    Alarm,
    /// Block or day finished: rising arpeggio.
    Done,
    /// Warning (access ends soon, pause reminder): single ping.
    Ping,
}

impl Sound {
    pub fn parse(s: &str) -> Option<Sound> {
        Some(match s {
            "break" => Sound::BreakStart,
            "alarm" => Sound::Alarm,
            "done" => Sound::Done,
            "ping" => Sound::Ping,
            _ => return None,
        })
    }
}

const RATE: u32 = 44_100;

/// One bell strike: fundamental + inharmonic partials with exponential decay.
fn bell(buf: &mut [f32], start: f32, freq: f32, len: f32, amp: f32) {
    let s0 = (start * RATE as f32) as usize;
    let n = (len * RATE as f32) as usize;
    let partials = [(1.0, 1.0, 1.0), (2.0, 0.45, 1.6), (2.76, 0.25, 2.4), (5.4, 0.08, 3.5)];
    for i in 0..n {
        let idx = s0 + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / RATE as f32;
        let attack = (t / 0.004).min(1.0);
        let mut v = 0.0;
        for (mul, a, decay) in partials {
            v += (TAU * freq * mul * t).sin() * a * (-t * decay * 3.0 / len).exp();
        }
        buf[idx] += v * amp * attack * 0.5;
    }
}

fn render(sound: Sound) -> Vec<f32> {
    match sound {
        Sound::BreakStart => {
            let mut b = vec![0.0; (RATE as f32 * 1.6) as usize];
            bell(&mut b, 0.0, 784.0, 1.2, 0.55);
            bell(&mut b, 0.28, 587.3, 1.3, 0.5);
            b
        }
        Sound::Done => {
            let mut b = vec![0.0; (RATE as f32 * 2.2) as usize];
            for (i, f) in [523.25, 659.25, 783.99, 1046.5].iter().enumerate() {
                bell(&mut b, i as f32 * 0.14, *f, 1.6, 0.45);
            }
            b
        }
        Sound::Ping => {
            let mut b = vec![0.0; (RATE as f32 * 0.9) as usize];
            bell(&mut b, 0.0, 987.8, 0.8, 0.5);
            b
        }
        Sound::Alarm => {
            // Classic mechanical alarm: rapid hammer strikes on two bells, 3 bursts.
            let mut b = vec![0.0; (RATE as f32 * 3.2) as usize];
            for burst in 0..3 {
                let t0 = burst as f32 * 1.05;
                for k in 0..14 {
                    let t = t0 + k as f32 * 0.052;
                    let f = if k % 2 == 0 { 1318.5 } else { 1174.7 };
                    bell(&mut b, t, f, 0.18, 0.95);
                }
            }
            b
        }
    }
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let gain = if peak > 0.98 { 0.98 / peak } else { 1.0 };
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s * gain).clamp(-1.0, 1.0);
        out.extend_from_slice(&((v * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

fn cached(sound: Sound) -> &'static [u8] {
    static BREAK: OnceLock<Vec<u8>> = OnceLock::new();
    static ALARM: OnceLock<Vec<u8>> = OnceLock::new();
    static DONE: OnceLock<Vec<u8>> = OnceLock::new();
    static PING: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = match sound {
        Sound::BreakStart => &BREAK,
        Sound::Alarm => &ALARM,
        Sound::Done => &DONE,
        Sound::Ping => &PING,
    };
    cell.get_or_init(|| wav(&render(sound)))
}

pub fn warm_up() {
    for s in [Sound::BreakStart, Sound::Alarm, Sound::Done, Sound::Ping] {
        let _ = cached(s);
    }
}

#[cfg(windows)]
pub fn play(sound: Sound) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    let data = cached(sound);
    // Buffers are 'static, so async playback may keep reading them.
    unsafe {
        PlaySoundW(data.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub fn play(sound: Sound) {
    let _ = cached(sound);
    eprintln!("[sound] {sound:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header() {
        let w = cached(Sound::Alarm);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert!(w.len() > 44 + RATE as usize * 2);
    }
}
