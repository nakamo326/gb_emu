use crate::bootrom::Bootrom;
use crate::cartridge::Cartridge;
use crate::hram::HRam;
use crate::ppu::Ppu;
use crate::timer::Timer;
use crate::wram::WRam;

pub struct Mmu {
    pub bootrom: Bootrom,
    pub cartridge: Option<Cartridge>,
    pub wram: WRam,
    pub hram: HRam,
    pub ppu: Ppu,
    pub timer: Timer,
    /// 割り込みフラグ (0xFF0F)
    pub if_: u8,
    /// 割り込み許可 (0xFFFF)
    pub ie: u8,
    /// シリアルデータ (0xFF01)
    serial_data: u8,
}

impl Mmu {
    pub fn new() -> Self {
        let bootrom = Bootrom::new("dmg_bootrom.bin");
        let wram = WRam::new();
        let hram = HRam::new();
        let ppu = Ppu::new();
        let timer = Timer::new();

        Self {
            bootrom,
            cartridge: None,
            wram,
            hram,
            ppu,
            timer,
            if_: 0,
            ie: 0,
            serial_data: 0,
        }
    }

    pub fn load_cartridge(&mut self, rom_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let cartridge = Cartridge::new(rom_path)?;
        self.cartridge = Some(cartridge);
        Ok(())
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF => {
                if self.bootrom.is_active() {
                    self.bootrom.read(addr)
                } else if let Some(cartridge) = &self.cartridge {
                    cartridge.read(addr)
                } else {
                    0xFF
                }
            }
            0x0100..=0x7FFF | 0xA000..=0xBFFF => {
                if let Some(cartridge) = &self.cartridge {
                    cartridge.read(addr)
                } else {
                    0xFF
                }
            }
            0x8000..=0x9FFF => self.ppu.read(addr),
            0xFE00..=0xFE9F => self.ppu.read(addr),
            0xFF00 => 0xFF, // ジョイパッド（未実装、全ボタン未押下）
            0xFF01 => self.serial_data,
            0xFF02 => 0x7E, // シリアル制御（転送完了）
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_ | 0xE0, // 上位3bitは常に1
            0xFF40..=0xFF4B => self.ppu.read(addr),
            0xC000..=0xFDFF => self.wram.read(addr),
            0xFF50 => 0xFF,
            0xFF80..=0xFFFE => self.hram.read(addr),
            0xFFFF => self.ie,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => {
                if let Some(cartridge) = &mut self.cartridge {
                    cartridge.write(addr, val);
                }
            }
            0x8000..=0x9FFF => self.ppu.write(addr, val),
            0xFE00..=0xFE9F => self.ppu.write(addr, val),
            0xFF00 => {} // ジョイパッド（未実装）
            0xFF01 => self.serial_data = val,
            0xFF02 => {
                // シリアル転送: bit7 がセットされたら文字を出力（blargg テスト用）
                if val & 0x80 != 0 {
                    print!("{}", self.serial_data as char);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.if_ = val & 0x1F,
            0xFF40..=0xFF4B => self.ppu.write(addr, val),
            0xC000..=0xFDFF => self.wram.write(addr, val),
            0xFF50 => self.bootrom.write(addr, val),
            0xFF80..=0xFFFE => self.hram.write(addr, val),
            0xFFFF => self.ie = val,
            _ => {}
        }
    }
}
