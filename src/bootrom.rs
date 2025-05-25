use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub struct Bootrom {
    rom: [u8; 0x100],
    active: bool,
}

impl Bootrom {
    fn read_file_to_array<P: AsRef<Path>>(path: P) -> io::Result<[u8; 0x100]> {
        let mut file = File::open(path)?;
        let mut buffer = [0; 0x100];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    pub fn new(path: &str) -> Self {
        let rom = Self::read_file_to_array(path).unwrap();
        Self { rom, active: true }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn write(&mut self, _: u16, val: u8) {
        self.active &= val == 0;
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.rom[addr as usize]
    }
}
