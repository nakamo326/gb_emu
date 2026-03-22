mod apu;
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
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");

    // --headless 以外の引数をROMパスとして扱う
    let rom_path = args.iter().skip(1).find(|a| *a != "--headless").map(|s| s.as_str());

    let backend: Box<dyn backend::Backend> = if headless {
        Box::new(backend::NullBackend)
    } else {
        Box::new(lcd::Lcd::new())
    };

    let mut gameboy = gameboy::GameBoy::new(backend, headless);

    if let Some(path) = rom_path {
        match gameboy.load_cartridge(path) {
            Ok(_) => println!("Loaded: {}", path),
            Err(e) => {
                eprintln!("Failed to load '{}': {}", path, e);
                std::process::exit(1);
            }
        }
    } else if gameboy.load_cartridge("test_rom.gb").is_ok() {
        println!("Loaded: test_rom.gb");
    } else {
        println!("No ROM found, running without cartridge");
    }

    gameboy.run();
}
