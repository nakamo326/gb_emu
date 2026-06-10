#![no_std]
#![no_main]

mod audio;
mod cartridge;
mod display;
mod input;

use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::board;
use cortex_m_rt::entry;

use gb_core::{
    bootrom::Bootrom,
    gameboy::GameBoy,
    mmu::Mmu,
    platform::NullAudio,
};

// NullAudio を使用中。SAI 実装後は audio::PcmAudio に差し替える。
use cartridge::GpioCart;
use display::Ili9341Display;
use input::GpioInput;

/// Teensy 4.1 ピン割り当て:
///
/// ILI9341 ディスプレイ (GPIO2):
///   SPI (LPSPI4): MOSI=11, MISO=12, SCK=13, CS=10
///   DC: pin 9, RST: pin 8, BL: pin 7
///
/// GB カートリッジバス:
///   D0-D7: pin 14-17, 22, 23, 40, 41 (GPIO1[18-25])
///   A0-A15: TODO (GPIO1/3/4, 実機で確認)
///   /RD: pin 33 (GPIO4[7])
///   /WR: pin 34 (GPIO2[28], t41 拡張)
#[entry]
fn main() -> ! {
    let board::Resources {
        lpspi4,
        mut gpio1,
        mut gpio2,
        mut gpio4,
        pins,
        ..
    } = board::t41(board::instances());

    // ------- ILI9341 ディスプレイ -------

    // IOMUXC を SPI ピンに設定し SPI を 24 MHz で初期化
    let spi: board::Lpspi4 = board::lpspi(
        lpspi4,
        board::LpspiPins {
            sdo: pins.p11,
            sdi: pins.p12,
            sck: pins.p13,
            pcs0: pins.p10,
        },
        24_000_000,
    );

    // バックライト有効化
    let mut bl = gpio2.output(pins.p7);
    let _ = embedded_hal::digital::v2::OutputPin::set_high(&mut bl);

    // DC / RST ピンを IOMUXC で GPIO に設定
    let dc  = gpio2.output(pins.p9);
    let rst = gpio2.output(pins.p8);
    let display = Ili9341Display::new(spi, dc, rst);

    // ------- GB カートリッジバス -------

    // IOMUXC をデータ・アドレス・制御ピンの GPIO モードに設定
    // (Output<P> を drop しても IOMUXC 設定は維持される)

    // D0-D7: GPIO1[18-25]
    drop(gpio1.output(pins.p14)); // D0 = GPIO1[18]
    drop(gpio1.output(pins.p15)); // D1 = GPIO1[19]
    drop(gpio1.output(pins.p40)); // D2 = GPIO1[20]
    drop(gpio1.output(pins.p41)); // D3 = GPIO1[21]
    drop(gpio1.output(pins.p17)); // D4 = GPIO1[22]
    drop(gpio1.output(pins.p16)); // D5 = GPIO1[23]
    drop(gpio1.output(pins.p22)); // D6 = GPIO1[24]
    drop(gpio1.output(pins.p23)); // D7 = GPIO1[25]

    // /RD: GPIO4[7] (pin 33)
    drop(gpio4.output(pins.p33));
    // /WR: GPIO2[28] (pin 34, t41 拡張)
    drop(gpio2.output(pins.p34));

    // TODO: A0-A15 のピンを追加設定 (GpioCart::new 内で without_pin で GDIR のみ設定)

    // Safety: 上記で IOMUXC 設定済み、board::t41() によるクロックゲート有効化済み
    let cart = unsafe { GpioCart::new(gpio1, gpio4, gpio2) };

    // ------- GB コア -------

    // BootROM は Flash に埋め込む（著作権注意: 配布不可）
    // let bootrom = Bootrom::from_bytes(*include_bytes!("../dmg_bootrom.bin"));
    let bootrom = Bootrom::disabled();

    let input = GpioInput::new();
    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, display, NullAudio, input);

    loop {
        gb.step();
    }
}
