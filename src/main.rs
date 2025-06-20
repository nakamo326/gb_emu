mod bootrom;
mod cpu;
mod gameboy;
mod hram;
mod lcd;
mod mmu;
mod ppu;
mod wram;

pub fn main() {
    let mut gameboy = gameboy::GameBoy::new();

    gameboy.run();
}
