# STM32F411E-DISCO onboard audio

Updated 2026-11-14. Firmware and host tests are under `src/`; generated artifacts
and the workspace-local compiler are under ignored `target/`. `Cargo.lock`
remains unchanged.

## Implementation status

- The release firmware cross-compiles and links for `thumbv7em-none-eabihf`.
- All 48 library host tests pass: nine audio/codec tests, 37 upstream Embassy
  ring-buffer tests, and two startup-specific checks.
- The host WAV exporter runs; its header and all 96,000 stereo frames were
  independently checked using Python's `wave` and `struct` modules.
- No board is present. Codec response, actual clocks, interrupt behavior, and
  audible playback remain unverified. Compilation is not a hardware test.

## Files

| File | Responsibility |
| --- | --- |
| `src/main.rs` | MCU clocks, reset, I2C adapter, I2S/DMA, startup, error handling |
| `src/codec.rs` | CS43L22 initialization and playback control; scripted bus tests |
| `src/audio.rs` | Shared fixed stereo sine generator and continuity/format tests |
| `src/lib.rs` | Hardware-independent, `no_std` library |
| `src/render_wav.rs` | Desktop-only `render-wav` Cargo example |
| `src/tests/dma.rs` | Host DMA-ring startup regression tests and minimal shim |
| `src/tests/upstream/` | Pinned Embassy ring-buffer fixture, provenance, and license |
| `.cargo/config.toml` | Embedded target/linker flags and corrected probe target |
| `Cargo.toml` | Desktop example; MCU dependencies restricted to bare-metal ARM |

The old external-PCM5102/stm32f4xx-hal `main.rs` has been replaced. No external
DAC board or jumper wires are needed for the new onboard audio path.

## Architecture and pins

The F411 MCU itself has no internal analog DAC. The Discovery board carries
an external CS43L22 DAC/headphone amplifier:

```text
PCM samples -> RAM ring -> DMA1 -> SPI3/I2S3 -> CS43L22 -> headphone jack
                                                ^
                                      I2C1 control + GPIO reset
```

| Function | MCU resource |
| --- | --- |
| Audio data | PC12, SPI3/I2S3 SD, AF6 |
| Stereo frame clock | PA4, I2S3 WS, AF6 |
| Serial bit clock | PC10, I2S3 CK, AF6 |
| Master clock | PC7, I2S3 MCK, AF6 |
| Control clock/data | PB6/PB9, I2C1, AF4 |
| Active-low codec reset | PD4 |
| Transmit DMA | DMA1 stream 7, request channel 0 |

Embassy's `DMA1_CH7` token names the stream, not ST request channel 7.
The codec's 7-bit I2C address is `0x4a`; ST HAL uses the shifted form `0x94`.
The probe-rs target is `STM32F411VETx`, confirmed in upstream target definitions.

## Audio format and clocks

The MCU is master; the codec is slave. The transport uses Philips I2S,
16-bit data in 16-bit slots, stereo `[left, right, ...]` interleaving, with
MCLK enabled. Cast signed `i16` samples to `u16` to preserve their two's-complement
bits. Zero is silence: do not add an unsigned analog-DAC midpoint.

The left tone has a 64-frame period; the right has a 32-frame period. At nominal
48 kHz they are 750 Hz and 1500 Hz. Their PCM peak is 4096 (~-18 dBFS). The
codec adds -30 dB master attenuation. These settings are conservative starting
levels, not a guarantee of acoustic safety for every headphone/load.

For initial bring-up, the firmware deliberately uses the internal 16 MHz HSI,
avoiding assumptions about a particular board revision's HSE crystal/bypass wiring:

- Main PLL M=16, N=192, P=2: SYSCLK/AHB 96 MHz, below the F411's 100 MHz limit.
- APB1 /2 = 48 MHz; APB2 /1 = 96 MHz.
- PLLI2S M=16, N=258, R=3: kernel clock 86 MHz.
- Embassy chooses I2SDIV=3, ODD=1 for nominal 48 kHz with MCLK enabled.
- `Fs = 86 MHz / (256 * 7) ~= 47,991.071 Hz`, before HSI tolerance.
- MCLK ~= 12.285714 MHz; BCLK ~= 1.535714 MHz; LRCLK ~= 47.991 kHz.

The actual tone frequencies therefore differ slightly even before HSI tolerance.
Use a confirmed crystal/external clock configuration for more accurate audio later.
The original linked Embassy example's 180 MHz SYSCLK is not legal for an F411.

## Startup sequence and source corrections

1. Hold PD4 low for 5 ms, release it, and wait 5 ms (ST board-level reset margins).
2. Read ID register `0x01`; require `(id & 0xf8) == 0xe0` to allow revision bits.
3. Keep power-down asserted and all outputs muted while configuring headphone-only
   power selection, clock auto-detection, Philips I2S, unity PCM/headphone gain,
   and -30 dB master volume.
4. Apply the required reserved-register initialization from Cirrus DS792F2 section
   4.11, preserving all other bits of register `0x32` while toggling bit 7.
5. Explicitly queue a full ring of silence before starting I2S/DMA.
6. Start MCLK/BCLK/LRCLK while the codec is still powered down.
7. Power up, enable digital soft ramp, and unmute headphones; keep speakers muted.
8. Continuously generate/refill tone blocks. Do not delay or log in the refill loop.

Important corrections to the earlier notes/ST example:

- Use clock register `0x05 = 0x80`: auto-detection without MCLK divide-by-two,
  matching the 256x MCLK relationship. ST's `0x81` is not copied blindly.
- Include the reserved sequence `0x00=0x99`, `0x47=0x80`, read `0x32`, set bit 7,
  clear bit 7, `0x00=0x00`. The fetched ST component driver omits this explicitly
  required datasheet sequence. Other bits in `0x32` must be preserved.
- Interface register `0x06=0x07` selects slave Philips I2S. Its AWL field is
  documented for DSP/right-justified formats, not as a required I2S word-length
  selector; the STM32 also explicitly selects the transmitted word length.
- PCM volume registers remain at unity, not ST's +5 dB initialization setting.

### Why the explicit silence prefill matters

At the locked Embassy revision, a newly constructed writable ring is logically
empty, even when its backing allocation contains zeroes. Starting DMA and then
performing codec I2C writes without first queuing data makes the DMA read index
overtake the write index. The first producer write returns `Overrun`.

Firmware queues four 512-word blocks before starting DMA. The 2048-word ring
then has ~21.3 ms of silence available; half/full transfer interrupts provide
~10.7 ms wakeup intervals. The three normal 100 kHz playback-control writes
take roughly 0.8 ms on the wire. Each has a 10 ms error timeout; on any error,
the code asserts codec reset and panics instead of continuing stale audio.

I2C setup is intentionally blocking and occurs before steady-state playback.
Only I2S uses DMA. No async I2C driver, stream-recovery framework, or extra task
is required for this first tone. The code fails closed on DMA errors rather
than attempting unverified mid-stream recovery/channel realignment.

The codec has no software stop API in this minimal example. If adding one, follow
DS792F2 section 4.10: mute, disable transition effects, power down, retain MCLK
for at least 100 microseconds, then remove clocks. Do not equate panic/reset
handling with a validated pop-free shutdown.

## Reproduce the checks

Commands below run from the repository root. The desktop target shown is this
machine's `aarch64-apple-darwin`; substitute another installed host target elsewhere.
Build outputs and generated audio stay under ignored `target/`.

```sh
cargo fmt --all -- --check
cargo test --locked --lib --target aarch64-apple-darwin
cargo run --locked --example render-wav --target aarch64-apple-darwin -- target/tone.wav
```

`tone.wav` is two seconds of the exact generated PCM, without the codec's
additional -30 dB attenuation. Listen at low host volume. The exporter overwrites
the output path supplied explicitly on the command line.

With a normal Rust toolchain that has the embedded target installed:

```sh
cargo build --locked --release --bin sinn2 --target thumbv7em-none-eabihf
```

### This machine's toolchain workaround

The installed Homebrew rustc 1.99.1 lacks the ARM standard library; neither
`rustup` nor `probe-rs` is installed. An upstream ARM library alone is incompatible
with that Homebrew compiler despite the matching version/hash display.

For verification, upstream rustc 1.99.1 plus matching host and ARM libraries were
downloaded from `static.rust-lang.org`, checked against SHA-256 hashes in the
official 1.99.1 channel manifest. The compiler is now kept **only** in
`target/toolchain`; temporary archives and unpacked installation packages have
been removed. It is a disposable build aid, not a tracked project dependency.
No global install, shell profile, or system toolchain was changed. Rebuild with:

```sh
RUSTC="$PWD/target/toolchain/bin/rustc" cargo build --locked --release --bin sinn2 --target thumbv7em-none-eabihf
```

The resulting `target/thumbv7em-none-eabihf/release/sinn2` was
confirmed as a statically linked 32-bit ARM EABI5 ELF executable.

### Embassy ring-buffer tests

`src/tests/upstream/embassy_ringbuffer.rs` is an unmodified copy from the locked
revision, with its license and provenance retained alongside it. `src/tests/dma.rs`
supplies only a `Word` bound and fake DMA positions, allowing the actual ring
algorithm and its upstream tests to execute with the other `cargo test --lib`
host tests. The shim and fixture are excluded from production firmware builds.

The added checks reproduce startup failure without prefill and resume the first
tone-write future across a simulated half-transfer interrupt after prefill.
This is not an emulation of STM32 registers, DMA timing, or the entire firmware.
Firmware, test sources, and this guide are trackable. The generated WAV,
compiler, and build outputs are ignored under `target/`. `cargo clean` will
remove the local compiler too; use a standard Rust toolchain with the ARM target
installed to avoid depending on that disposable copy.

## Hardware flight-test checklist

1. Start without headphones on your ears; retain low digital and host/amplifier levels.
2. Flash/run using the F411VE target; confirm there is no codec-ID/I2C panic.
3. Verify PC7 MCLK, PC10 BCLK, and PA4 LRCLK against the approximate values above.
4. Check PC12 data and the one-bit Philips I2S delay if the clocks exist but audio fails.
5. Confirm the left/right tones are distinct and sustained without a DMA panic.
6. Only then consider volume changes, more accurate HSE clocks, stopping/restarting,
   file playback, microphone input, or other synthesizer features.

## Emulation verdict

Stock QEMU and Renode do not provide the F411 I2S + circular DMA + CS43L22 audio
path. QEMU documents DMA/I2C/I2S gaps for its STM32 models. Renode's STM32 SPI
model explicitly rejects I2S mode, its F4 Discovery platform has no codec attached,
and its STM32 DMA model lacks the required streaming behavior. This finding is
from source inspection, not emulator execution. Building missing models would be
a separate project; host PCM/control tests are the practical no-board route.

## Primary references

- [Cirrus CS43L22 DS792F2](https://statics.cirrus.com/pubs/proDatasheet/CS43L22_F2.pdf): sections 4.9–4.11 and 7.
- [ST board control wiring/reset](https://github.com/STMicroelectronics/32f411ediscovery-bsp/tree/c7de631adeb20ecd2097581ab22e88d316a8bdb3)
- [ST board audio setup and pin definitions](https://github.com/STMicroelectronics/32f411ediscovery-bsp/blob/c7de631adeb20ecd2097581ab22e88d316a8bdb3/stm32f411e_discovery_audio.c)
- [ST component driver](https://github.com/STMicroelectronics/stm32-cs43l22/blob/main/cs43l22.c)
- [Embassy I2S at locked revision](https://github.com/embassy-rs/embassy/blob/b2c49cceb6a3c600494347ebd5c0cdb6a163f04a/embassy-stm32/src/i2s.rs)
- [Embassy ring buffer at locked revision](https://github.com/embassy-rs/embassy/blob/b2c49cceb6a3c600494347ebd5c0cdb6a163f04a/embassy-stm32/src/dma/ringbuffer.rs)
- [probe-rs target names](https://github.com/probe-rs/probe-rs/blob/master/probe-rs/targets/STM32F4_Series.yaml)
- [QEMU STM32 support](https://www.qemu.org/docs/master/system/arm/stm32.html)
- [Renode STM32 SPI](https://github.com/renode/renode-infrastructure/blob/master/src/Emulator/Peripherals/Peripherals/SPI/STM32SPI.cs)
- [Renode STM32 DMA](https://github.com/renode/renode-infrastructure/blob/master/src/Emulator/Peripherals/Peripherals/DMA/STM32DMA.cs)

The user's ST UM1842 and AN3997 endpoints failed to load in the initial research
session; they have not been claimed as read. AN3997 concerns the older F4DISCOVERY,
so its clock recipe must not be assumed correct for the F411.
