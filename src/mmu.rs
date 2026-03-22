use crate::bootrom::Bootrom;
use crate::cartridge::Cartridge;
use crate::hram::HRam;
use crate::input::ButtonState;
use crate::joypad::Joypad;
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
    pub joypad: Joypad,
    /// 割り込みフラグ (0xFF0F)
    pub if_: u8,
    /// 割り込み許可 (0xFFFF)
    pub ie: u8,
    /// シリアルデータ (0xFF01)
    serial_data: u8,
    /// シリアル出力バッファ（テスト終了検知用）
    serial_buf: String,
    /// テスト終了フラグ
    pub test_done: bool,
    /// 外部RAM監視バッファ ($A000-$A003 + テキスト)
    ram_monitor: [u8; 4],
    ram_text_buf: Vec<u8>,
    ram_sig_ok: bool,
}

impl Mmu {
    pub fn new() -> Self {
        let bootrom = Bootrom::new("dmg_bootrom.bin");
        let wram = WRam::new();
        let hram = HRam::new();
        let ppu = Ppu::new();
        let timer = Timer::new();
        let joypad = Joypad::new();

        Self {
            bootrom,
            cartridge: None,
            wram,
            hram,
            ppu,
            timer,
            joypad,
            if_: 0,
            ie: 0,
            serial_data: 0,
            serial_buf: String::new(),
            test_done: false,
            ram_monitor: [0x80, 0, 0, 0],
            ram_text_buf: Vec::new(),
            ram_sig_ok: false,
        }
    }

    /// BootROM をスキップして DMG 起動直後のハードウェアレジスタ値をセットする
    pub fn apply_dmg_init(&mut self) {
        // PPU
        self.write(0xFF40, 0x91); // LCDC
        self.write(0xFF41, 0x85); // STAT
        self.write(0xFF47, 0xFC); // BGP
        self.write(0xFF48, 0xFF); // OBP0
        self.write(0xFF49, 0xFF); // OBP1
        // タイマー
        self.write(0xFF05, 0x00); // TIMA
        self.write(0xFF06, 0x00); // TMA
        self.write(0xFF07, 0x00); // TAC
        // 割り込み
        self.if_ = 0xE1;
        self.ie = 0x00;
    }

    /// ボタン状態を更新し、新たに押下があれば Joypad 割り込みフラグをセットする
    pub fn update_joypad(&mut self, state: &ButtonState) {
        if self.joypad.update(state) {
            self.if_ |= 0x10; // Joypad 割り込み
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
            0xFF00 => self.joypad.read(),
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
                // blargg v2テスト: 外部RAMへの結果書き込みを監視
                if addr >= 0xA000 && addr <= 0xA003 {
                    self.ram_monitor[(addr - 0xA000) as usize] = val;
                    // シグネチャ確認: $A001=$DE, $A002=$B0, $A003=$61
                    if self.ram_monitor[1] == 0xDE && self.ram_monitor[2] == 0xB0 && self.ram_monitor[3] == 0x61 {
                        self.ram_sig_ok = true;
                    }
                    // $A000 != 0x80 かつシグネチャあり = テスト完了
                    if self.ram_sig_ok && addr == 0xA000 && val != 0x80 {
                        use std::io::Write;
                        let text = String::from_utf8_lossy(&self.ram_text_buf).into_owned();
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                        self.test_done = true;
                    }
                } else if addr >= 0xA004 && addr < 0xB000 {
                    // テキスト出力バッファに追記
                    if val != 0 {
                        self.ram_text_buf.push(val);
                    }
                }
            }
            0x8000..=0x9FFF => self.ppu.write(addr, val),
            0xFE00..=0xFE9F => self.ppu.write(addr, val),
            0xFF00 => self.joypad.write(val),
            0xFF01 => self.serial_data = val,
            0xFF02 => {
                // シリアル転送: bit7 がセットされたら文字を出力（blargg テスト用）
                if val & 0x80 != 0 {
                    let c = self.serial_data as char;
                    print!("{}", c);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    self.serial_buf.push(c);
                    // blargg テストは "Passed" または "Failed" で終了
                    if self.serial_buf.contains("Passed") || self.serial_buf.contains("Failed") {
                        self.test_done = true;
                    }
                }
            }
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.if_ = val & 0x1F,
            0xFF46 => {
                // OAM DMA転送: src_base * 0x100 から 0xFE00 へ 160バイトコピー
                let src = (val as u16) << 8;
                for i in 0..0xA0u16 {
                    let byte = self.read(src + i);
                    self.ppu.write(0xFE00 + i, byte);
                }
            }
            0xFF40..=0xFF4B => self.ppu.write(addr, val),
            0xC000..=0xFDFF => self.wram.write(addr, val),
            0xFF50 => self.bootrom.write(addr, val),
            0xFF80..=0xFFFE => self.hram.write(addr, val),
            0xFFFF => self.ie = val,
            _ => {}
        }
    }
}
