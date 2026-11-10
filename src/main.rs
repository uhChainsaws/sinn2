#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_time::Timer;
use {defmt_rtt as _, panic_halt as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("Button demo - polling user button to cycle LEDs");

    // Настраиваем кнопку PA0 как вход с подтяжкой к земле
    let button = Input::new(p.PA0, Pull::Down);

    // Настраиваем 4 светодиода как выходы
    let mut led_orange = Output::new(p.PD13, Level::Low, Speed::Low);
    let mut led_green  = Output::new(p.PD12, Level::Low, Speed::Low);
    let mut led_red    = Output::new(p.PD14, Level::Low, Speed::Low);
    let mut led_blue   = Output::new(p.PD15, Level::Low, Speed::Low);

    let mut state = 0u8;
    let mut last_pressed = false;

    loop {
        // is_high() возвращает true, когда кнопка нажата (Pull::Down)
        let pressed = button.is_high();

        if pressed && !last_pressed {
            info!("Button pressed!");

            // Выключаем все светодиоды
            led_orange.set_low();
            led_green.set_low();
            led_red.set_low();
            led_blue.set_low();

            match state {
                0 => { info!("Orange LED"); led_orange.set_high(); }
                1 => { info!("Green LED");  led_green.set_high();  }
                2 => { info!("Red LED");    led_red.set_high();    }
                3 => { info!("Blue LED");   led_blue.set_high();   }
                _ => { info!("All LEDs");
                       led_orange.set_high();
                       led_green.set_high();
                       led_red.set_high();
                       led_blue.set_high(); }
            }

            state = (state + 1) % 5;
        }

        last_pressed = pressed;
        Timer::after_millis(50).await;
    }
}