pub const SAMPLE_RATE: u32 = 48_000;
/// Peak magnitude of the fixed wavetable, not a configurable gain control.
pub const AMPLITUDE: i16 = 4096;
pub type StereoFrame = [u16; 2];

const SINE: [i16; 64] = [
    0, 402, 799, 1189, 1567, 1931, 2276, 2598, 2896, 3166, 3406, 3612, 3784, 3920, 4017, 4076,
    AMPLITUDE, 4076, 4017, 3920, 3784, 3612, 3406, 3166, 2896, 2598, 2276, 1931, 1567, 1189, 799,
    402, 0, -402, -799, -1189, -1567, -1931, -2276, -2598, -2896, -3166, -3406, -3612, -3784,
    -3920, -4017, -4076, -AMPLITUDE, -4076, -4017, -3920, -3784, -3612, -3406, -3166, -2896, -2598,
    -2276, -1931, -1567, -1189, -799, -402,
];

/// Generates 750 Hz left and 1500 Hz right at 48 kHz, as two's-complement samples.
pub struct Tone {
    phase: usize,
}

impl Tone {
    pub const fn new() -> Self {
        Self { phase: 0 }
    }

    pub fn fill(&mut self, frames: &mut [StereoFrame]) {
        for frame in frames {
            *frame = [SINE[self.phase] as u16, SINE[(self.phase * 2) & 63] as u16];
            self.phase = (self.phase + 1) & 63;
        }
    }
}

impl Default for Tone {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_signed_zero_centered_and_bounded() {
        let mut frames = [[0; 2]; 64];
        Tone::new().fill(&mut frames);

        for channel in 0..2 {
            let mut sum = 0i32;
            let mut minimum = i16::MAX;
            let mut maximum = i16::MIN;
            for frame in &frames {
                let sample = frame[channel] as i16;
                assert!((-AMPLITUDE..=AMPLITUDE).contains(&sample));
                sum += i32::from(sample);
                minimum = minimum.min(sample);
                maximum = maximum.max(sample);
            }
            assert_eq!(sum, 0);
            assert_eq!(minimum, -AMPLITUDE);
            assert_eq!(maximum, AMPLITUDE);
        }
        assert_eq!(frames[0], [0, 0]);
        assert_eq!(frames[16][0], 0x1000);
        assert_eq!(frames[48][0], 0xf000);
        assert_eq!(frames[24][1], 0xf000);
    }

    #[test]
    fn stereo_channels_have_distinct_fundamental_periods() {
        let mut frames = [[0; 2]; 128];
        Tone::new().fill(&mut frames);

        for (channel, period) in [(0, 64), (1, 32)] {
            for index in 0..frames.len() - period {
                assert_eq!(frames[index][channel], frames[index + period][channel]);
            }
            for shorter in 1..period {
                assert!(
                    (0..period).any(|index| {
                        frames[index][channel] != frames[index + shorter][channel]
                    })
                );
            }
        }
        assert_eq!(frames[8][1] as i16, AMPLITUDE);
        assert_eq!(frames[16][0] as i16, AMPLITUDE);
        assert_ne!(frames[8][0], frames[8][1]);
    }

    #[test]
    fn uneven_and_empty_blocks_preserve_continuity() {
        let mut expected = [[0; 2]; 128];
        Tone::new().fill(&mut expected);

        let mut actual = [[0; 2]; 128];
        let mut tone = Tone::default();
        let mut offset = 0;
        for size in [0, 1, 7, 0, 65, 3, 52, 0] {
            tone.fill(&mut actual[offset..offset + size]);
            offset += size;
        }
        assert_eq!(offset, actual.len());
        assert_eq!(actual, expected);

        let mut next = [[0; 2]; 1];
        tone.fill(&mut next);
        assert_eq!(next[0], expected[0]);
    }

    #[test]
    fn repeated_periods_match_after_many_wraps() {
        let mut period = [[0; 2]; 64];
        let mut tone = Tone::new();
        tone.fill(&mut period);

        let mut repeated = [[0; 2]; 256];
        for _ in 0..8 {
            tone.fill(&mut repeated);
            for block in repeated.chunks_exact(64) {
                assert_eq!(block, &period);
            }
        }
    }
}
