#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    
    let mut led_orange = Output::new(p.PD13, Level::Low, Speed::Low);
    let mut led_green  = Output::new(p.PD12, Level::Low, Speed::Low);
    let mut led_red    = Output::new(p.PD14, Level::Low, Speed::Low);
    let mut led_blue   = Output::new(p.PD15, Level::Low, Speed::Low);

    let mut count = 0u8;

    loop {
            let state = (31 - count).min(count);
            count = (count + 1) % 32;
            info!("[ STATE ]: {}", state);

            led_green.set_level((state >> 0 & 1 == 1).into());
            led_orange.set_level((state >> 1 & 1 == 1).into());
            led_red.set_level((state >> 2 & 1 == 1).into());
            led_blue.set_level((state >> 3 & 1 == 1).into());

            Timer::after_millis(90).await;
    }
}