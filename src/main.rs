mod bootrom;
mod cpu;
mod gameboy;
mod hram;
mod lcd;
mod mmu;
mod ppu;
mod renderer;
mod wram;

pub fn main() {
    // let lcd = Box::new(lcd::Lcd::new());
    let mut renderer = Box::new(renderer::TerminalRenderer::new(160, 144));
    let mut gameboy = gameboy::GameBoy::new(renderer);

    gameboy.run();
}
