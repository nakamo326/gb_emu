#![no_std]
#![no_main]

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
    platform::{NullAudio, NullCartridge},
};

use display::Ili9341Display;
use input::GpioInput;

#[entry]
fn main() -> ! {
    // board::t41 の初期化（ペリフェラルを取得）
    let _resources = board::t41(board::instances());

    // BootROM は Flash に埋め込む（著作権注意: 配布不可）
    // let bootrom = Bootrom::from_bytes(*include_bytes!("../dmg_bootrom.bin"));
    let bootrom = Bootrom::disabled();

    let display = Ili9341Display::new();
    let input = GpioInput::new();

    let mmu = Mmu::new(bootrom, NullCartridge);
    let mut gb = GameBoy::new(mmu, display, NullAudio, input);

    loop {
        gb.step();
    }
}
