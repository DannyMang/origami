use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    fs::File,
    io::{ErrorKind, Seek, SeekFrom, Write},
    mem,
    path::Path,
    sync::{Arc, Mutex},
};

struct Tape<W> {
    file: W,
    frames: u32,
    peaks: Vec<f32>,
    top: f32,
    cut: bool,
}

impl<W: Write> Tape<W> {
    fn hear(&mut self, data: &[f32], channels: usize, rate: u32) {
        let raw: Vec<u8> = data
            .iter()
            .flat_map(|s| ((s.clamp(-1., 1.) * 32767.) as i16).to_le_bytes())
            .collect();
        let mut done = 0;
        while done < raw.len() && !self.cut {
            match self.file.write(&raw[done..]) {
                Ok(n) if n > 0 => done += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                _ => self.cut = true,
            }
        }
        for frame in data.chunks(channels).take(done / (channels * 2)) {
            self.top = frame.iter().fold(self.top, |m, s| m.max(s.abs()));
            self.frames += 1;
            if self.frames.is_multiple_of(rate / 20) {
                let top = mem::take(&mut self.top);
                self.peaks.push(top);
            }
        }
    }
}

pub struct Mic {
    stream: Option<cpal::Stream>,
    tape: Arc<Mutex<Tape<File>>>,
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

fn seal(file: &mut File, rate: u32, channels: u16, frames: u32) {
    let head = header(rate, channels, frames);
    let _ = file.set_len(head.len() as u64 + frames as u64 * channels as u64 * 2);
    let _ = file.seek(SeekFrom::Start(0)).and_then(|_| file.write_all(&head));
}

impl Mic {
    pub fn new(path: &Path, name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let named = |n| host.input_devices().ok()?.find(|d| d.name().is_ok_and(|x| x == n));
        let dev = name.and_then(named).or_else(|| host.default_input_device());
        let dev = dev.context("no microphone found")?;
        let config = dev.default_input_config()?;
        let (rate, channels) = (config.sample_rate().0, config.channels());
        let mut file = File::create_new(path)?;
        file.write_all(&header(rate, channels, 0))?;
        let tape = Arc::new(Mutex::new(Tape {
            file,
            frames: 0,
            peaks: vec![],
            top: 0.,
            cut: false,
        }));
        let heard = tape.clone();
        let stream = dev.build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| heard.lock().unwrap().hear(data, channels as usize, rate),
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

    pub fn cut(&self) -> bool {
        self.tape.lock().unwrap().cut
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stream.take();
        let mut t = self.tape.lock().unwrap();
        let frames = t.frames;
        seal(&mut t.file, self.rate, self.channels, frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io};

    struct Short(File, usize);

    impl Write for Short {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let n = data.len().min(self.1);
            self.1 -= n;
            match n {
                0 => Err(ErrorKind::StorageFull.into()),
                _ => self.0.write(&data[..n]),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // When the disk fills up part way through a recording, only the sound that reached the file is
    // counted, recording stops there, and the finished file says exactly how much sound it holds.
    #[test]
    fn full_disk_cuts_the_take() {
        let path = std::env::temp_dir().join("origami-mic-test.wav");
        let _ = fs::remove_file(&path);
        let mut file = File::create_new(&path).unwrap();
        file.write_all(&header(48000, 2, 0)).unwrap();
        let mut t = Tape {
            file: Short(file, 9610),
            frames: 0,
            peaks: vec![],
            top: 0.,
            cut: false,
        };
        for _ in 0..3 {
            t.hear(&[0.5; 2400], 2, 48000);
        }
        assert_eq!((t.frames, t.cut, t.peaks.len()), (2402, true, 1));
        seal(&mut t.file.0, 48000, 2, t.frames);
        let wav = fs::read(&path).unwrap();
        assert_eq!(wav.len(), 44 + 9608);
        assert_eq!(wav[40..44], 9608u32.to_le_bytes());
        assert_eq!(wav[4..8], (36 + 9608u32).to_le_bytes());
    }
}
