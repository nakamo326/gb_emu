mod bootrom;
mod cartridge;
mod cpu;
mod gameboy;
mod hram;
mod lcd;
mod mmu;
mod ppu;
mod renderer;
mod wram;

pub fn main() {
    let lcd = Box::new(lcd::Lcd::new());
    // let mut renderer = Box::new(renderer::TerminalRenderer::new(160, 144));
    let mut gameboy = gameboy::GameBoy::new(lcd);

    // Load test ROM if available
    if let Ok(_) = gameboy.load_cartridge("test_rom.gb") {
        println!("Test ROM loaded successfully");
    } else if let Ok(_) = gameboy.load_cartridge("cpu_instrs.gb") {
        println!("CPU instruction test ROM loaded");
    } else {
        println!("No test ROM found, running without cartridge");
    }

    gameboy.run();
}
