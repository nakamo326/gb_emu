use gb_core::platform::CartridgeBus;
use std::fs;

#[derive(Debug, Clone, Copy)]
pub enum CartridgeType {
    RomOnly = 0x00,
    Mbc1 = 0x01,
    Mbc1Ram = 0x02,
    Mbc1RamBattery = 0x03,
    Mbc3 = 0x11,
    Mbc3Ram = 0x12,
    Mbc3RamBattery = 0x13,
    Mbc3Timer = 0x0F,
    Mbc3TimerRam = 0x10,
    Mbc5 = 0x19,
    Mbc5Ram = 0x1A,
    Mbc5RamBattery = 0x1B,
}

impl From<u8> for CartridgeType {
    fn from(value: u8) -> Self {
        match value {
            0x00 => CartridgeType::RomOnly,
            0x01 => CartridgeType::Mbc1,
            0x02 => CartridgeType::Mbc1Ram,
            0x03 => CartridgeType::Mbc1RamBattery,
            0x0F => CartridgeType::Mbc3Timer,
            0x10 => CartridgeType::Mbc3TimerRam,
            0x11 => CartridgeType::Mbc3,
            0x12 => CartridgeType::Mbc3Ram,
            0x13 => CartridgeType::Mbc3RamBattery,
            0x19 => CartridgeType::Mbc5,
            0x1A => CartridgeType::Mbc5Ram,
            0x1B => CartridgeType::Mbc5RamBattery,
            _ => CartridgeType::RomOnly,
        }
    }
}

pub trait MemoryBankController {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
}

pub struct RomOnly {
    rom: Vec<u8>,
}

impl RomOnly {
    pub fn new(rom: Vec<u8>) -> Self {
        Self { rom }
    }
}

impl MemoryBankController for RomOnly {
    fn read(&self, addr: u16) -> u8 {
        if (addr as usize) < self.rom.len() {
            self.rom[addr as usize]
        } else {
            0xFF
        }
    }

    fn write(&mut self, _addr: u16, _value: u8) {
        // ROM only cartridges don't support writing
    }
}

pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    mode: bool, // false = ROM banking mode, true = RAM banking mode
    rom_size: usize,
    ram_size: usize,
}

impl Mbc1 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        let rom_size = rom.len();
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mode: false,
            rom_size,
            ram_size,
        }
    }
}

impl MemoryBankController for Mbc1 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                let bank = if self.mode && self.rom_size > 0x80000 {
                    (self.ram_bank << 5) & 0x60
                } else {
                    0
                };
                let offset = (bank as usize * 0x4000) + addr as usize;
                if offset < self.rom_size {
                    self.rom[offset]
                } else {
                    0xFF
                }
            }
            0x4000..=0x7FFF => {
                let mut bank = self.rom_bank;
                if self.mode && self.rom_size > 0x80000 {
                    bank |= (self.ram_bank << 5) & 0x60;
                }
                if bank == 0 {
                    bank = 1;
                }
                let offset = (bank as usize * 0x4000) + (addr as usize - 0x4000);
                if offset < self.rom_size {
                    self.rom[offset]
                } else {
                    0xFF
                }
            }
            0xA000..=0xBFFF => {
                if self.ram_enabled && self.ram_size > 0 {
                    let bank = if self.mode { self.ram_bank } else { 0 };
                    let offset = (bank as usize * 0x2000) + (addr as usize - 0xA000);
                    if offset < self.ram_size {
                        self.ram[offset]
                    } else {
                        0xFF
                    }
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let bank = value & 0x1F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x03;
            }
            0x6000..=0x7FFF => {
                self.mode = (value & 0x01) != 0;
            }
            0xA000..=0xBFFF => {
                if self.ram_enabled && self.ram_size > 0 {
                    let bank = if self.mode { self.ram_bank } else { 0 };
                    let offset = (bank as usize * 0x2000) + (addr as usize - 0xA000);
                    if offset < self.ram_size {
                        self.ram[offset] = value;
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct Mbc3 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    rom_size: usize,
    ram_size: usize,
}

impl Mbc3 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        let rom_size = rom.len();
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            rom_size,
            ram_size,
        }
    }
}

impl MemoryBankController for Mbc3 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if (addr as usize) < self.rom_size { self.rom[addr as usize] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                let bank = if self.rom_bank == 0 { 1 } else { self.rom_bank };
                let offset = (bank as usize * 0x4000) + (addr as usize - 0x4000);
                if offset < self.rom_size { self.rom[offset] } else { 0xFF }
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled { return 0xFF; }
                match self.ram_bank {
                    0x00..=0x03 => {
                        let offset = (self.ram_bank as usize * 0x2000) + (addr as usize - 0xA000);
                        if offset < self.ram_size { self.ram[offset] } else { 0xFF }
                    }
                    0x08..=0x0C => 0x00, // RTC レジスタ（スタブ）
                    _ => 0xFF,
                }
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (value & 0x0F) == 0x0A,
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            0x6000..=0x7FFF => {} // RTC ラッチ（未実装）
            0xA000..=0xBFFF => {
                if !self.ram_enabled { return; }
                if self.ram_bank <= 0x03 {
                    let offset = (self.ram_bank as usize * 0x2000) + (addr as usize - 0xA000);
                    if offset < self.ram_size { self.ram[offset] = value; }
                }
            }
            _ => {}
        }
    }
}

pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank: u16,
    ram_bank: u8,
    ram_enabled: bool,
    rom_size: usize,
    ram_size: usize,
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        let rom_size = rom.len();
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            rom_size,
            ram_size,
        }
    }
}

impl MemoryBankController for Mbc5 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if (addr as usize) < self.rom_size { self.rom[addr as usize] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                let offset = (self.rom_bank as usize * 0x4000) + (addr as usize - 0x4000);
                if offset < self.rom_size { self.rom[offset] } else { 0xFF }
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled || self.ram_size == 0 { return 0xFF; }
                let offset = (self.ram_bank as usize * 0x2000) + (addr as usize - 0xA000);
                if offset < self.ram_size { self.ram[offset] } else { 0xFF }
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (value & 0x0F) == 0x0A,
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | (value as u16),
            0x3000..=0x3FFF => self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 8),
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            0xA000..=0xBFFF => {
                if !self.ram_enabled || self.ram_size == 0 { return; }
                let offset = (self.ram_bank as usize * 0x2000) + (addr as usize - 0xA000);
                if offset < self.ram_size { self.ram[offset] = value; }
            }
            _ => {}
        }
    }
}

pub struct Cartridge {
    mbc: Box<dyn MemoryBankController>,
    header: CartridgeHeader,
}

#[derive(Debug)]
pub struct CartridgeHeader {
    pub title: String,
    pub cartridge_type: CartridgeType,
    pub rom_size: u8,
    pub ram_size: u8,
    /// ROM ヘッダ 0x0143: 0x80=CGB 対応、0xC0=CGB 専用、その他=DMG
    pub cgb_flag: u8,
}

impl Cartridge {
    pub fn new(rom_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rom = fs::read(rom_path)?;
        
        if rom.len() < 0x150 {
            return Err("ROM file too small".into());
        }

        let header = CartridgeHeader {
            // 0x0143 は CGB フラグのためタイトルは 0x134..0x143（15 バイト）まで
            title: String::from_utf8_lossy(&rom[0x134..0x143])
                .trim_end_matches('\0')
                .to_string(),
            cartridge_type: CartridgeType::from(rom[0x147]),
            rom_size: rom[0x148],
            ram_size: rom[0x149],
            cgb_flag: rom[0x0143],
        };

        let ram_size = match header.ram_size {
            0x00 => 0,
            0x01 => 0x800,    // 2KB
            0x02 => 0x2000,   // 8KB
            0x03 => 0x8000,   // 32KB
            0x04 => 0x20000,  // 128KB
            0x05 => 0x10000,  // 64KB
            _ => 0,
        };

        let mbc: Box<dyn MemoryBankController> = match header.cartridge_type {
            CartridgeType::RomOnly => Box::new(RomOnly::new(rom)),
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                Box::new(Mbc1::new(rom, ram_size))
            }
            CartridgeType::Mbc3
            | CartridgeType::Mbc3Ram
            | CartridgeType::Mbc3RamBattery
            | CartridgeType::Mbc3Timer
            | CartridgeType::Mbc3TimerRam => Box::new(Mbc3::new(rom, ram_size)),
            CartridgeType::Mbc5 | CartridgeType::Mbc5Ram | CartridgeType::Mbc5RamBattery => {
                Box::new(Mbc5::new(rom, ram_size))
            }
        };

        Ok(Self { mbc, header })
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.mbc.read(addr)
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.mbc.write(addr, value);
    }

    pub fn header(&self) -> &CartridgeHeader {
        &self.header
    }
}

impl CartridgeBus for Cartridge {
    fn read(&self, addr: u16) -> u8 {
        self.mbc.read(addr)
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mbc.write(addr, val);
    }
}