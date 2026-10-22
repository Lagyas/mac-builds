//! Audio output through cpal (WASAPI / CoreAudio / ALSA natively, Web Audio
//! in the browser). The synth is shared with the UI behind a mutex; the
//! callback renders straight into the device buffer.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hs2_core::Synth;
use std::sync::{Arc, Mutex};

pub type Shared = Arc<Mutex<Synth>>;

pub struct Audio {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub device: String,
}

/// Output sample rate of the default device (before the stream is opened).
pub fn default_rate() -> Option<u32> {
    let dev = cpal::default_host().default_output_device()?;
    Some(dev.default_output_config().ok()?.sample_rate().0)
}

pub fn start(synth: Shared) -> Result<Audio, String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no audio output device")?;
    let name = dev.name().unwrap_or_else(|_| "audio".into());
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let sr = cfg.sample_rate().0;
    let ch = cfg.channels() as usize;
    let fmt = cfg.sample_format();
    let mut config: cpal::StreamConfig = cfg.into();
    // modest fixed buffer where the device allows it (low latency for playing)
    config.buffer_size = cpal::BufferSize::Default;
    let err = |e| eprintln!("audio: {e}");
    let mut l = vec![0.0f32; 8192];
    let mut r = vec![0.0f32; 8192];
    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let synth = synth.clone();
            dev.build_output_stream(
                &config,
                move |out: &mut [$t], _| {
                    let frames = out.len() / ch;
                    if l.len() < frames {
                        l.resize(frames, 0.0);
                        r.resize(frames, 0.0);
                    }
                    match synth.lock() {
                        Ok(mut s) => s.render(&mut l[..frames], &mut r[..frames]),
                        Err(_) => {
                            l[..frames].fill(0.0);
                            r[..frames].fill(0.0);
                        }
                    }
                    for (i, f) in out.chunks_mut(ch).enumerate() {
                        for (c, v) in f.iter_mut().enumerate() {
                            let x = if c % 2 == 0 { l[i] } else { r[i] };
                            *v = $conv(x);
                        }
                    }
                },
                err,
                None,
            )
        }};
    }
    let stream = match fmt {
        cpal::SampleFormat::F32 => build!(f32, |x: f32| x),
        cpal::SampleFormat::I16 => build!(i16, |x: f32| (x.clamp(-1.0, 1.0) * 32767.0) as i16),
        cpal::SampleFormat::U16 => build!(u16, |x: f32| ((x.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65535.0) as u16),
        f => return Err(format!("unsupported sample format {f:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Audio { _stream: stream, sample_rate: sr, device: name })
}
