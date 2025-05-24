pub struct Bootrom {
    rom: [u8; 0x100],
    active: bool,
}

impl Bootrom {
    pub fn new(rom: [u8; 0x100]) -> Self {
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
