use crate::export::audio_args;
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use origami_lang::{Timeline, ffmpeg};
use std::{
    collections::VecDeque,
    io::Read,
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
    thread,
    time::Duration,
};

pub struct Player {
    _stream: cpal::Stream,
    child: Child,
    played: Arc<AtomicU64>,
    fed: Arc<AtomicU64>,
    eof: Arc<AtomicBool>,
    rate: f64,
    t0: f64,
    speed: f64,
}

impl Player {
    pub fn warm() {
        let _ = cpal::default_host().default_output_device().map(|d| d.default_output_config());
    }

    pub fn new(tl: &Timeline, t: f64, speed: f64) -> Result<Self> {
        let dev = cpal::default_host().default_output_device().context("no audio device")?;
        let rate = dev.default_output_config()?.sample_rate();
        let mut child = ffmpeg()
            .args(["-v", "error"])
            .args(audio_args(tl, t, rate.0, speed))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("ffmpeg not found")?;
        let mut out = child.stdout.take().unwrap();
        let buf = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let (played, fed, eof) = (
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        thread::spawn({
            let (buf, fed, eof) = (buf.clone(), fed.clone(), eof.clone());
            move || {
                let mut b = [0; 8192];
                while Arc::strong_count(&buf) > 1 && out.read_exact(&mut b).is_ok() {
                    buf.lock()
                        .unwrap()
                        .extend(b.chunks_exact(4).map(|s| f32::from_le_bytes(s.try_into().unwrap())));
                    fed.fetch_add(b.len() as u64 / 8, Relaxed);
                    while Arc::strong_count(&buf) > 1 && buf.lock().unwrap().len() > rate.0 as usize * 2 {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
                eof.store(true, Relaxed);
            }
        });
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let count = played.clone();
        let stream = dev.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut b = buf.lock().unwrap();
                let n = data.len().min(b.len());
                data[..n].iter_mut().zip(b.drain(..n)).for_each(|(d, s)| *d = s);
                data[n..].fill(0.);
                count.fetch_add(n as u64 / 2, Relaxed);
            },
            |_| {},
            None,
        )?;
        stream.play()?;
        Ok(Self {
            _stream: stream,
            child,
            played,
            fed,
            eof,
            rate: rate.0 as f64,
            t0: t,
            speed,
        })
    }

    pub fn time(&self) -> f64 {
        self.t0 + self.played.load(Relaxed) as f64 / self.rate * self.speed
    }

    pub fn done(&self) -> bool {
        self.eof.load(Relaxed) && self.played.load(Relaxed) >= self.fed.load(Relaxed)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
