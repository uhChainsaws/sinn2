pub const SAMPLE_RATE: u32 = 48_000;
/// Maximum magnitude of the averaged chord, matching the fixed wavetable peak.
pub const AMPLITUDE: i16 = 4096;
pub type StereoFrame = [u16; 2];

const SINE: [i16; 64] = [
    0, 402, 799, 1189, 1567, 1931, 2276, 2598, 2896, 3166, 3406, 3612, 3784, 3920, 4017, 4076,
    AMPLITUDE, 4076, 4017, 3920, 3784, 3612, 3406, 3166, 2896, 2598, 2276, 1931, 1567, 1189, 799,
    402, 0, -402, -799, -1189, -1567, -1931, -2276, -2598, -2896, -3166, -3406, -3612, -3784,
    -3920, -4017, -4076, -AMPLITUDE, -4076, -4017, -3920, -3784, -3612, -3406, -3166, -2896, -2598,
    -2276, -1931, -1567, -1189, -799, -402,
];

// Equal temperament, A4 = 440 Hz: G3, B3, D4, F4, rounded to millihertz.
const NOTE_MILLIHZ: [u32; 4] = [195_998, 246_942, 293_665, 349_228];
const PHASE_STEPS: [u32; 4] = [
    phase_step(NOTE_MILLIHZ[0]),
    phase_step(NOTE_MILLIHZ[1]),
    phase_step(NOTE_MILLIHZ[2]),
    phase_step(NOTE_MILLIHZ[3]),
];

const fn phase_step(frequency_millihz: u32) -> u32 {
    let denominator = SAMPLE_RATE as u64 * 1000;
    ((((frequency_millihz as u64) << 32) + denominator / 2) / denominator) as u32
}

fn sine_sample(phase: u32) -> i32 {
    let index = (phase >> 26) as usize;
    let fraction = ((phase >> 10) & 0xffff) as i32;
    let first = i32::from(SINE[index]);
    let next = i32::from(SINE[(index + 1) & 63]);
    // Interpolation avoids coarse pitch steps without a larger table or float math.
    first + (((next - first) * fraction) >> 16)
}

/// Sustained G3–B3–D4–F4, mixed equally into both channels at nominal 48 kHz.
pub struct G7Chord {
    phases: [u32; 4],
}

impl G7Chord {
    pub const fn new() -> Self {
        Self { phases: [0; 4] }
    }

    /// Output silence while gated off; retain oscillator phases for the next press.
    pub fn fill_gated(&mut self, frames: &mut [StereoFrame], playing: bool) {
        if playing {
            self.fill(frames);
        } else {
            frames.fill([0; 2]);
        }
    }

    pub fn fill(&mut self, frames: &mut [StereoFrame]) {
        for frame in frames {
            let mut sum = 0i32;
            for (phase, step) in self.phases.iter_mut().zip(PHASE_STEPS) {
                sum += sine_sample(*phase);
                *phase = phase.wrapping_add(step);
            }
            // Average rather than sum: four voices must not quadruple the peak level.
            let sample = (sum / 4) as i16 as u16;
            *frame = [sample; 2];
        }
    }
}

impl Default for G7Chord {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_channels_have_the_same_bounded_signed_mix() {
        let mut chord = G7Chord::new();
        let mut frames = [[0; 2]; 256];
        let mut sum = 0i64;
        let mut minimum = i16::MAX;
        let mut maximum = i16::MIN;
        for _ in 0..375 {
            chord.fill(&mut frames);
            for frame in &frames {
                assert_eq!(frame[0], frame[1]);
                let sample = frame[0] as i16;
                assert!((-AMPLITUDE..=AMPLITUDE).contains(&sample));
                sum += i64::from(sample);
                minimum = minimum.min(sample);
                maximum = maximum.max(sample);
            }
        }
        assert!(minimum < -1024 && maximum > 1024);
        // The chord does not have an integer-length period in this two-second window.
        assert!((sum as f64 / 96_000.0).abs() < 4.0);
    }

    #[test]
    fn interpolation_preserves_extrema_and_wrap_continuity() {
        assert_eq!(sine_sample(0), 0);
        assert_eq!(sine_sample(1 << 30), i32::from(AMPLITUDE));
        assert_eq!(sine_sample(1 << 31), 0);
        assert_eq!(sine_sample(3 << 30), -i32::from(AMPLITUDE));
        assert!(sine_sample(u32::MAX).abs() <= 1);
        assert_eq!((-AMPLITUDE) as u16, 0xf000);
    }

    #[test]
    fn phase_steps_match_equal_tempered_g7_notes() {
        for (midi_note, step) in [55, 59, 62, 65].into_iter().zip(PHASE_STEPS) {
            let expected = 440.0 * 2.0f64.powf(f64::from(midi_note - 69) / 12.0);
            let actual = f64::from(step) * f64::from(SAMPLE_RATE) / 4_294_967_296.0;
            assert!((actual - expected).abs() < 0.001);
        }
    }

    #[test]
    fn mix_matches_an_independent_four_sine_reference() {
        let frequencies =
            [55, 59, 62, 65].map(|note| 440.0 * 2.0f64.powf(f64::from(note - 69) / 12.0));
        let mut frames = [[0; 2]; 4800];
        G7Chord::new().fill(&mut frames);
        assert_eq!(frames[0], [0, 0]);
        for (index, frame) in frames.iter().enumerate() {
            let time = index as f64 / f64::from(SAMPLE_RATE);
            let expected: f64 = frequencies
                .iter()
                .map(|frequency| (std::f64::consts::TAU * frequency * time).sin())
                .sum::<f64>()
                * f64::from(AMPLITUDE)
                / 4.0;
            assert!((f64::from(frame[0] as i16) - expected).abs() < 8.0);
        }
    }

    #[test]
    fn uneven_and_empty_blocks_preserve_continuity() {
        let mut expected = [[0; 2]; 129];
        G7Chord::new().fill(&mut expected);

        let mut actual = [[0; 2]; 128];
        let mut chord = G7Chord::default();
        let mut offset = 0;
        for size in [0, 1, 7, 0, 65, 3, 52, 0] {
            chord.fill(&mut actual[offset..offset + size]);
            offset += size;
        }
        assert_eq!(offset, actual.len());
        assert_eq!(actual, expected[..128]);

        let mut next = [[0; 2]; 1];
        chord.fill(&mut next);
        assert_eq!(next[0], expected[128]);
    }

    #[test]
    fn each_voice_wraps_phase_without_overflow_or_reset() {
        let initial = [u32::MAX - 10; 4];
        let mut chord = G7Chord { phases: initial };
        chord.fill(&mut [[0; 2]; 8]);
        for (index, step) in PHASE_STEPS.into_iter().enumerate() {
            assert_eq!(chord.phases[index], initial[index].wrapping_add(step * 8));
        }
    }

    #[test]
    fn gate_off_clears_stale_samples_and_holds_all_phases() {
        let mut chord = G7Chord::new();
        let mut frames = [[0xffff; 2]; 256];
        chord.fill_gated(&mut frames, false);
        assert_eq!(frames, [[0; 2]; 256]);
        assert_eq!(chord.phases, [0; 4]);

        chord.fill_gated(&mut frames, true);
        assert!(frames.iter().any(|frame| frame[0] != 0));
        let phases = chord.phases;
        chord.fill_gated(&mut frames, false);
        assert_eq!(frames, [[0; 2]; 256]);
        assert_eq!(chord.phases, phases);
    }

    #[test]
    fn press_release_press_resumes_the_same_chord_in_both_channels() {
        let mut expected = [[0; 2]; 46];
        G7Chord::new().fill(&mut expected);

        let mut chord = G7Chord::new();
        let mut first = [[0; 2]; 17];
        chord.fill_gated(&mut first, true);
        assert_eq!(first, expected[..17]);

        let mut silence = [[0xffff; 2]; 256];
        for _ in 0..8 {
            chord.fill_gated(&mut silence, false);
            assert_eq!(silence, [[0; 2]; 256]);
        }

        let mut second = [[0; 2]; 29];
        chord.fill_gated(&mut second, true);
        assert_eq!(second, expected[17..]);
    }
}
