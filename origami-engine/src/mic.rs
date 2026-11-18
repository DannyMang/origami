use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    mem,
    path::Path,
    sync::{Arc, Mutex},
};

struct Tape {
    file: BufWriter<File>,
    frames: u32,
    peaks: Vec<f32>,
    top: f32,
}

pub struct Mic {
    stream: Option<cpal::Stream>,
    tape: Arc<Mutex<Tape>>,
    rate: u32,
    channels: u16,
}

pub fn inputs() -> Vec<String> {
    let host = cpal::default_host();
    let name = |d: cpal::Device| d.name().ok();
    let first = host.default_input_device().and_then(name);
    let rest = host.input_devices().into_iter().flatten().filter_map(name);
    first.iter().cloned().chain(rest.filter(|n| Some(n) != first.as_ref())).collect()
}

fn header(rate: u32, channels: u16, frames: u32) -> Vec<u8> {
    let (block, bytes) = (channels * 2, frames * channels as u32 * 2);
    [
        b"RIFF".as_slice(),
        &(36 + bytes).to_le_bytes(),
        b"WAVEfmt \x10\0\0\0\x01\0",
        &channels.to_le_bytes(),
        &rate.to_le_bytes(),
        &(rate * block as u32).to_le_bytes(),
        &block.to_le_bytes(),
        b"\x10\0data",
        &bytes.to_le_bytes(),
    ]
    .concat()
}

impl Mic {
    pub fn new(path: &Path, name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let named = |n| host.input_devices().ok()?.find(|d| d.name().is_ok_and(|x| x == n));
        let dev = name.and_then(named).or_else(|| host.default_input_device());
        let dev = dev.context("no microphone found")?;
        let config = dev.default_input_config()?;
        let (rate, channels) = (config.sample_rate().0, config.channels());
        let mut file = BufWriter::new(File::create_new(path)?);
        file.write_all(&header(rate, channels, 0))?;
        let tape = Arc::new(Mutex::new(Tape {
            file,
            frames: 0,
            peaks: vec![],
            top: 0.,
        }));
        let heard = tape.clone();
        let stream = dev.build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut t = heard.lock().unwrap();
                for frame in data.chunks(channels as usize) {
                    for s in frame {
                        t.top = t.top.max(s.abs());
                        let _ = t.file.write_all(&((s.clamp(-1., 1.) * 32767.) as i16).to_le_bytes());
                    }
                    t.frames += 1;
                    if t.frames.is_multiple_of(rate / 20) {
                        let top = mem::take(&mut t.top);
                        t.peaks.push(top);
                    }
                }
            },
            |_| {},
            None,
        )?;
        stream.play()?;
        Ok(Self {
            stream: Some(stream),
            tape,
            rate,
            channels,
        })
    }

    pub fn secs(&self) -> f64 {
        self.tape.lock().unwrap().frames as f64 / self.rate as f64
    }

    pub fn peaks(&self) -> Vec<f32> {
        self.tape.lock().unwrap().peaks.clone()
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stream.take();
        let mut t = self.tape.lock().unwrap();
        let head = header(self.rate, self.channels, t.frames);
        let _ = t.file.seek(SeekFrom::Start(0)).and_then(|_| t.file.write_all(&head));
        let _ = t.file.flush();
    }
}
