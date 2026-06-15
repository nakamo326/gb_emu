#![no_std]
#![no_main]

mod audio;
mod cartridge;
mod display;
mod input;
mod sdcard;

use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::board;

use gb_core::{
    bootrom::Bootrom,
    gameboy::GameBoy,
    mmu::Mmu,
    platform::NullAudio,
};

use display::Ili9341Display;
use input::GpioInput;
use sdcard::FlashCart;

/// Teensy 4.1 ピン割り当て:
///
/// ILI9341 ディスプレイ (LPSPI4):
///   MOSI=11, MISO=12, SCK=13, CS=10(PCS0), DC=9, RST=8
///
/// 実機検証で判明した配線の重要事項 (詳細は docs/teensy_setup_guide.md):
///   - バックライト(LED/BL)は GPIO では電流不足で駆動不可 → 3.3V に直結する。
///     下の p7 set_high は無害だが実際のバックライト点灯には寄与しない。
///   - 単一 SPI デバイスなら CS→GND, RESET→3.3V 固定が最も確実 (その場合 p10/p8 は未使用)。
///
/// ROM は Flash に埋め込み (include_bytes!)。
/// SDカード対応は docs/teensy_setup_guide.md を参照。

// ROM は Flash に埋め込む。ビルド前に roms/game.gb を配置すること。
static ROM: &[u8] = include_bytes!("../../roms/game.gb");

#[bsp::rt::entry]
fn main() -> ! {
    let board::Resources {
        lpspi4,
        mut gpio2,
        pins,
        ..
    } = board::t41(board::instances());

    // ------- ROM (Flash 埋め込み) -------
    let cart = FlashCart::new(ROM);

    // ------- ILI9341 ディスプレイ (LPSPI4) -------
    let spi: board::Lpspi4 = board::lpspi(
        lpspi4,
        board::LpspiPins {
            sdo: pins.p11,
            sdi: pins.p12,
            sck: pins.p13,
            pcs0: pins.p10,
        },
        24_000_000, // 描画高速化のため 24MHz。配線が崩れる場合は下げる
    );

    // バックライト有効化
    let mut bl = gpio2.output(pins.p7);
    let _ = embedded_hal::digital::v2::OutputPin::set_high(&mut bl);

    let dc  = gpio2.output(pins.p9);
    let rst = gpio2.output(pins.p8);
    let display = Ili9341Display::new(spi, dc, rst);

    // ------- GB コア -------

    // BootROM を使う場合: Bootrom::from_bytes(*include_bytes!("../dmg_bootrom.bin"))
    // 著作権注意 — 配布不可
    let bootrom = Bootrom::disabled();

    let input = GpioInput::new();
    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, display, NullAudio, input);

    loop {
        gb.step();
    }
}
