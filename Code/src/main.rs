#![no_std]
#![no_main]
//is it worth fixing the start vscode bug?
use defmt_rtt as _;
use hal::pwm::tim3;
use panic_probe as _;
use stm32f3xx_hal as hal;

use hal::delay::Delay;
use hal::pac;
use hal::prelude::*;
use hal::spi::Spi;

use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use mfrc522::{Mfrc522, Uid};

#[cortex_m_rt::entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let mut gpioc = dp.GPIOC.split(&mut rcc.ahb);
    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut gpiod = dp.GPIOD.split(&mut rcc.ahb);
    // clock configuration
    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(48.MHz())
        .pclk1(24.MHz())
        .freeze(&mut flash.acr);

    //tim3 clock
    let tim3_channels = tim3(
        dp.TIM3,
        1280,    // resolution of duty cycle
        50.Hz(), // frequency of period
        &clocks, // To get the timer's clock speed
    );

    //pwm pins and tim3

    let pa6 = gpioa 
        .pa6
        .into_af_push_pull::<2>(&mut gpioa.moder, &mut gpioa.otyper, &mut gpioa.afrl);

    let mut tim3_ch1 = tim3_channels.0.output_to_pa6(pa6);

    // Configure pins for SPI
    let nss = gpiod //nss/sda =pd0
        .pd0
        .into_push_pull_output(&mut gpiod.moder, &mut gpiod.otyper);

    let sck = gpioc         //sck = pc10
        .pc10
        .into_af_push_pull(&mut gpioc.moder, &mut gpioc.otyper, &mut gpioc.afrh);

    let miso = gpioc            //miso = pc11
        .pc11
        .into_af_push_pull(&mut gpioc.moder, &mut gpioc.otyper, &mut gpioc.afrh);
    let mosi = gpioc            //mosi = pc12
        .pc12
        .into_af_push_pull(&mut gpioc.moder, &mut gpioc.otyper, &mut gpioc.afrh);

    // defaults to using MODE_0
    let spi = Spi::new(dp.SPI3, (sck, miso, mosi), 1.MHz(), clocks, &mut rcc.apb1);

    let mut timer = Delay::new(cp.SYST, clocks);

    let mut mfrc522 = Mfrc522::new(spi, nss).expect("could not create MFRC522");
    match mfrc522.version() {
        Ok(version) => defmt::info!("version {:x}", version),
        Err(_) => defmt::error!("version error"),
    }

    //this Loop needs understanding

    let write = false;
    loop {
        //turning servo left and right 
        tim3_ch1.set_duty(tim3_ch1.get_max_duty()/10); // 5% duty cyle
        tim3_ch1.enable();
        cortex_m::asm::delay(5_000_000);
        tim3_ch1.set_duty(tim3_ch1.get_max_duty()/20); // 10% duty cyle
        tim3_ch1.enable();
        cortex_m::asm::delay(5_000_000);

        //verification for RFID 
        match mfrc522.wupa() {
            Ok(atqa) => {
                defmt::info!("new card detected");
                match mfrc522.select(&atqa) {

                    Ok(ref uid @ Uid::Single(ref inner)) => {
                        defmt::info!("card uid {=[?]}", inner.as_bytes());
                        handle_card(&mut mfrc522, &uid, write);
                    }
                    Ok(ref uid @ Uid::Double(ref inner)) => {
                        defmt::info!("card double uid {=[?]}", inner.as_bytes());
                        handle_card(&mut mfrc522, &uid, write);
                    }
                    Ok(_) => {
                        defmt::info!("got other uid size")
                    }

                    Err(_) => {
                        //uid errors 
                        defmt::error!("Select error");
                       
                    }
                }
            }
            //wupa function errors 
            Err(_) => {
                defmt::error!("WUPA error");
            
            }
        }
        timer.delay_ms(1000u32);
    }
}

fn handle_card<E, SPI, NSS>(mfrc522: &mut Mfrc522<SPI, NSS>, uid: &Uid, write: bool)
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    NSS: OutputPin,
{
    let key = [0xFF; 6];
    let buffer = [
        0xDE, 0x42, 0xAD, 0x42, 0xBE, 0x42, 0xEF, 0x42, 0xCA, 0x42, 0xFE, 0x42, 0xBA, 0x42, 0xBE,
        0x42,
    ];
    if mfrc522.mf_authenticate(uid, 1, &key).is_ok() {
        if write {
            match mfrc522.mf_write(1, buffer) {
                Ok(_) => {
                    defmt::info!("write success");
                }
                Err(_) => {
                    defmt::error!("error during read");
                
                }
            }
        } else {
            match mfrc522.mf_read(1) {
                Ok(data) => defmt::info!("read {=[?]}", data),
                Err(_) => {
                    defmt::error!("error during read");

                }
            }
        }
    } else {
        defmt::warn!("Could not authenticate");
    }

    if mfrc522.hlta().is_err() {
        defmt::error!("Could not halt");
    }
    if mfrc522.stop_crypto1().is_err() {
        defmt::error!("Could not disable crypto1");
    }
}



#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}
