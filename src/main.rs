extern crate sdl2;

mod bootrom;
mod cpu;
mod gameboy;
mod hram;
mod lcd;
mod mmu;
mod ppu;
mod wram;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

fn read_file_to_boxed_slice<P: AsRef<Path>>(path: P) -> io::Result<[u8; 0x100]> {
    let mut file = File::open(path)?;
    let mut buffer = [0; 0x100];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

pub fn main() -> Result<(), String> {
    let bootrom = read_file_to_boxed_slice("dmg_bootrom.bin").unwrap();

    let bootrom = bootrom::Bootrom::new(bootrom);
    let mut gameboy = gameboy::GameBoy::new(bootrom);

    gameboy.run();

    Ok(())
}
