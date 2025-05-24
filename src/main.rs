extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use std::time::Duration;

mod bootrom;
mod cpu;
mod gameboy;
mod hram;
mod lcd;
mod peripherals;
mod ppu;
mod wram;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

fn read_file_to_boxed_slice<P: AsRef<Path>>(path: P) -> io::Result<Box<[u8]>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer.into_boxed_slice())
}

pub fn main() -> Result<(), String> {
    let bootrom = read_file_to_boxed_slice("dmg_bootrom.bin").unwrap();
    println!("ファイル読み込み成功！サイズ: {}バイト", bootrom.len());

    let bootrom = bootrom::Bootrom::new(bootrom);
    let mut gameboy = gameboy::GameBoy::new(bootrom);

    gameboy.run();

    Ok(())
}
