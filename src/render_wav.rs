use sinn2::audio::{SAMPLE_RATE, StereoFrame, Tone};
use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};

const FRAME_COUNT: u32 = SAMPLE_RATE * 2;
const DATA_BYTES: u32 = FRAME_COUNT * 4;

fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let path = args.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: render-wav <output.wav>",
        )
    })?;
    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: render-wav <output.wav>",
        ));
    }

    let mut output = BufWriter::new(File::create(path)?);
    output.write_all(b"RIFF")?;
    output.write_all(&(36 + DATA_BYTES).to_le_bytes())?;
    output.write_all(b"WAVEfmt ")?;
    output.write_all(&16u32.to_le_bytes())?;
    output.write_all(&1u16.to_le_bytes())?;
    output.write_all(&2u16.to_le_bytes())?;
    output.write_all(&SAMPLE_RATE.to_le_bytes())?;
    output.write_all(&(SAMPLE_RATE * 4).to_le_bytes())?;
    output.write_all(&4u16.to_le_bytes())?;
    output.write_all(&16u16.to_le_bytes())?;
    output.write_all(b"data")?;
    output.write_all(&DATA_BYTES.to_le_bytes())?;

    let mut tone = Tone::new();
    let mut frames: [StereoFrame; 1024] = [[0; 2]; 1024];
    let mut bytes = [0u8; 4096];
    let mut remaining = FRAME_COUNT as usize;
    while remaining != 0 {
        let count = remaining.min(frames.len());
        tone.fill(&mut frames[..count]);
        for (frame, encoded) in frames[..count].iter().zip(bytes.chunks_exact_mut(4)) {
            encoded[..2].copy_from_slice(&frame[0].to_le_bytes());
            encoded[2..].copy_from_slice(&frame[1].to_le_bytes());
        }
        output.write_all(&bytes[..count * 4])?;
        remaining -= count;
    }
    output.flush()
}
