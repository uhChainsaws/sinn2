//! Quiet stereo test tones through the STM32F411E-DISCO headphone jack.
//! PC12=SD, PA4=WS, PC10=CK, PC7=MCLK; PB6/PB9=I2C1; PD4=codec reset.

#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::{self, I2c, Master};
use embassy_stm32::i2s::{self, Format, I2S};
use embassy_stm32::mode::Blocking;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, dma, peripherals};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use sinn2::audio::{SAMPLE_RATE, Tone};
use sinn2::codec::{self, Cs43l22};

const DMA_WORDS: usize = 2048;
const BLOCK_FRAMES: usize = 256;

bind_interrupts!(struct Irqs {
    DMA1_STREAM7 => dma::InterruptHandler<peripherals::DMA1_CH7>;
});

struct CodecBus(I2c<'static, Blocking, Master>);

impl codec::ControlBus for CodecBus {
    type Error = i2c::Error;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.blocking_write(address, bytes)
    }

    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        result: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.0.blocking_write_read(address, bytes, result)
    }
}

fn clock_config() -> embassy_stm32::Config {
    use embassy_stm32::rcc::*;

    let mut config = embassy_stm32::Config::default();
    // HSI avoids depending on board-revision HSE wiring. Its tolerance affects pitch.
    config.rcc.pll_src = PllSource::Hsi;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::Div16,
        mul: PllMul::Mul192,
        divp: Some(PllPDiv::Div2), // 16 MHz / 16 * 192 / 2 = 96 MHz (F411 max: 100).
        divq: None,
        divr: None,
    });
    config.rcc.sys = Sysclk::Pll1P;
    config.rcc.ahb_pre = AHBPrescaler::Div1;
    config.rcc.apb1_pre = APBPrescaler::Div2;
    config.rcc.apb2_pre = APBPrescaler::Div1;
    config.rcc.plli2s = Some(Pll {
        prediv: PllPreDiv::Div16,
        mul: PllMul::Mul258,
        divp: None,
        divq: None,
        divr: Some(PllRDiv::Div3), // 86 MHz: Fs ~= 47991 Hz with 256x MCLK / divider 7.
    });
    config
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(clock_config());
    let mut reset = Output::new(p.PD4, Level::Low, Speed::Low);

    let mut control_config = i2c::Config::default();
    control_config.frequency = Hertz(100_000);
    // A failed control transaction must not stall startup indefinitely.
    control_config.timeout = Duration::from_millis(10);
    let control = I2c::new_blocking(p.I2C1, p.PB6, p.PB9, control_config);

    // ST's board-level reset margins exceed the codec's 1 ms minimum low pulse.
    Timer::after_millis(5).await;
    reset.set_high();
    Timer::after_millis(5).await;

    let mut codec = match Cs43l22::configure(CodecBus(control)) {
        Ok(codec) => codec,
        Err(error) => {
            reset.set_low();
            match error {
                codec::Error::Bus(error) => defmt::panic!("codec setup i2c error: {:?}", error),
                codec::Error::UnexpectedId(id) => {
                    defmt::panic!("unexpected codec id: {=u8:#x}", id)
                }
            }
        }
    };
    defmt::info!("codec configured; starting nominal 48 khz stereo, left 750 hz / right 1500 hz");

    let mut dma_buffer = [0u16; DMA_WORDS];
    let mut audio_config = i2s::Config::default();
    audio_config.frequency = Hertz(SAMPLE_RATE);
    audio_config.format = Format::Data16Channel16;
    audio_config.master_clock = true;
    let mut audio = I2S::new_txonly(
        p.SPI3,
        p.PC12,
        p.PA4,
        p.PC10,
        p.PC7,
        p.DMA1_CH7,
        Irqs,
        &mut dma_buffer,
        audio_config,
    );

    // A zero-filled allocation is not logically queued data in Embassy's TX ring.
    // Prime the full ring before DMA starts, giving codec startup ~21 ms of silence.
    let mut frames = [[0u16; 2]; BLOCK_FRAMES];
    for _ in 0..DMA_WORDS / (2 * BLOCK_FRAMES) {
        if let Err(error) = audio.write(frames.as_flattened()).await {
            reset.set_low();
            defmt::panic!("audio prefill error: {:?}", error);
        }
    }

    // Apply clocks and silence while PDN is still set, then power up and unmute.
    // Do not insert long awaits here: the running DMA ring needs timely refills.
    audio.start();
    if let Err(error) = codec.start() {
        reset.set_low();
        defmt::panic!("codec start i2c error: {:?}", error);
    }

    let mut tone = Tone::new();
    loop {
        tone.fill(&mut frames);
        if let Err(error) = audio.write(frames.as_flattened()).await {
            // Fail closed instead of silently replaying stale DMA data or swapping channels.
            reset.set_low();
            defmt::panic!("audio dma error: {:?}", error);
        }
    }
}
