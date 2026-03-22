mod backend;
mod bootrom;
mod cartridge;
mod cpu;
mod gameboy;
mod hram;
mod input;
mod joypad;
mod lcd;
mod mmu;
mod ppu;
mod renderer;
mod timer;
mod wram;

pub fn main() {
    let headless = std::env::args().any(|a| a == "--headless");

    let backend: Box<dyn backend::Backend> = if headless {
        Box::new(backend::NullBackend)
    } else {
        Box::new(lcd::Lcd::new())
    };

    let mut gameboy = gameboy::GameBoy::new(backend, headless);

    // blargg テスト ROM を優先ロード
    if gameboy.load_cartridge("blargg/instr_timing.gb").is_ok() {
        println!("Loaded: blargg/instr_timing.gb");
    } else if gameboy.load_cartridge("test_rom.gb").is_ok() {
        println!("Loaded: test_rom.gb");
    } else {
        println!("No ROM found, running without cartridge");
    }

    gameboy.run();
}
