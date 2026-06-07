use crate::apu::Apu;
use crate::bootrom::Bootrom;
use crate::hram::HRam;
use crate::input::ButtonState;
use crate::joypad::Joypad;
use crate::platform::CartridgeBus;
use crate::ppu::Ppu;
use crate::timer::Timer;
use crate::wram::WRam;

/// CPU から見たメモリバス抽象。`CartridgeBus` の具象型を型消去し、
/// CPU と命令テーブル(`ExecFn`)を非ジェネリックに保つためのトレイト。
pub trait MemoryBus {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn if_(&self) -> u8;
    fn set_if(&mut self, val: u8);
    fn ie(&self) -> u8;
}

pub struct Mmu<C: CartridgeBus> {
    pub bootrom: Bootrom,
    pub cart: C,
    pub wram: WRam,
    pub hram: HRam,
    pub ppu: Ppu,
    pub timer: Timer,
    pub joypad: Joypad,
    pub apu: Apu,
    /// 割り込みフラグ (0xFF0F)
    pub if_: u8,
    /// 割り込み許可 (0xFFFF)
    pub ie: u8,
    /// シリアルデータ (0xFF01)
    serial_data: u8,
    /// blargg テスト ROM の出力監視（host のみ）
    #[cfg(feature = "test-harness")]
    pub test: TestHarness,
}

impl<C: CartridgeBus> Mmu<C> {
    pub fn new(bootrom: Bootrom, cart: C) -> Self {
        Self {
            bootrom,
            cart,
            wram: WRam::new(),
            hram: HRam::new(),
            ppu: Ppu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            apu: Apu::new(),
            if_: 0,
            ie: 0,
            serial_data: 0,
            #[cfg(feature = "test-harness")]
            test: TestHarness::new(),
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
        // APU
        self.write(0xFF26, 0xF1); // NR52: 電源ON、CH1有効
        self.write(0xFF25, 0xF3); // NR51: パンニング
        self.write(0xFF24, 0x77); // NR50: マスターボリューム
        self.write(0xFF10, 0x80); // NR10: Sweep
        self.write(0xFF11, 0xBF); // NR11: Duty/Length
        self.write(0xFF12, 0xF3); // NR12: Envelope
        self.write(0xFF14, 0xBF); // NR14: Freq high + trigger
        self.write(0xFF16, 0x3F); // NR21
        self.write(0xFF17, 0x00); // NR22
        self.write(0xFF19, 0xBF); // NR24
        self.write(0xFF1A, 0x7F); // NR30: CH3 DAC
        self.write(0xFF1B, 0xFF); // NR31: Length
        self.write(0xFF1C, 0x9F); // NR32: Level
        self.write(0xFF1E, 0xBF); // NR34
        self.write(0xFF21, 0x00); // NR42: Envelope
        self.write(0xFF22, 0x00); // NR43: Frequency
        self.write(0xFF23, 0xBF); // NR44
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

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF => {
                if self.bootrom.is_active() {
                    self.bootrom.read(addr)
                } else {
                    self.cart.read(addr)
                }
            }
            0x0100..=0x7FFF | 0xA000..=0xBFFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.read(addr),
            0xFE00..=0xFE9F => self.ppu.read(addr),
            0xFF00 => self.joypad.read(),
            0xFF01 => self.serial_data,
            0xFF02 => 0x7E, // シリアル制御（転送完了）
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_ | 0xE0, // 上位3bitは常に1
            0xFF10..=0xFF3F => self.apu.read(addr),
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
                self.cart.write(addr, val);
                #[cfg(feature = "test-harness")]
                self.test.on_cart_write(addr, val);
            }
            0x8000..=0x9FFF => self.ppu.write(addr, val),
            0xFE00..=0xFE9F => self.ppu.write(addr, val),
            0xFF00 => self.joypad.write(val),
            0xFF01 => self.serial_data = val,
            0xFF02 => {
                // シリアル転送: bit7 がセットされたら文字を出力（blargg テスト用）
                if val & 0x80 != 0 {
                    #[cfg(feature = "test-harness")]
                    self.test.on_serial(self.serial_data);
                }
            }
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.if_ = val & 0x1F,
            0xFF10..=0xFF3F => self.apu.write(addr, val),
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

/// blargg テスト ROM のシリアル/外部RAM出力を監視するハーネス（host 専用）。
///
/// 標準出力は行わず、出力バイトを `serial_log` / `ram_text_buf` に蓄積し、
/// "Passed"/"Failed" 検出または外部RAMシグネチャで `test_done` を立てる。
/// host 側はこれらのバッファを読んで標準出力へ流す。
#[cfg(feature = "test-harness")]
pub struct TestHarness {
    /// シリアル(0xFF01/0xFF02)経由の出力ログ
    pub serial_log: heapless::Vec<u8, 8192>,
    /// 外部RAM経由(blargg v2)のテキスト出力
    pub ram_text_buf: heapless::Vec<u8, 8192>,
    /// テスト完了フラグ
    pub test_done: bool,
    /// 外部RAM監視バッファ ($A000-$A003)
    ram_monitor: [u8; 4],
    ram_sig_ok: bool,
}

#[cfg(feature = "test-harness")]
impl TestHarness {
    fn new() -> Self {
        Self {
            serial_log: heapless::Vec::new(),
            ram_text_buf: heapless::Vec::new(),
            test_done: false,
            ram_monitor: [0x80, 0, 0, 0],
            ram_sig_ok: false,
        }
    }

    fn on_serial(&mut self, byte: u8) {
        let _ = self.serial_log.push(byte);
        // blargg テストは "Passed" または "Failed" で終了
        if slice_ends_with(&self.serial_log, b"Passed")
            || slice_ends_with(&self.serial_log, b"Failed")
        {
            self.test_done = true;
        }
    }

    fn on_cart_write(&mut self, addr: u16, val: u8) {
        // blargg v2テスト: 外部RAMへの結果書き込みを監視
        if (0xA000..=0xA003).contains(&addr) {
            self.ram_monitor[(addr - 0xA000) as usize] = val;
            // シグネチャ確認: $A001=$DE, $A002=$B0, $A003=$61
            if self.ram_monitor[1] == 0xDE
                && self.ram_monitor[2] == 0xB0
                && self.ram_monitor[3] == 0x61
            {
                self.ram_sig_ok = true;
            }
            // $A000 != 0x80 かつシグネチャあり = テスト完了
            if self.ram_sig_ok && addr == 0xA000 && val != 0x80 {
                self.test_done = true;
            }
        } else if (0xA004..0xB000).contains(&addr) && val != 0 {
            // テキスト出力バッファに追記
            let _ = self.ram_text_buf.push(val);
        }
    }
}

#[cfg(feature = "test-harness")]
fn slice_ends_with(buf: &[u8], pat: &[u8]) -> bool {
    buf.len() >= pat.len() && &buf[buf.len() - pat.len()..] == pat
}

impl<C: CartridgeBus> MemoryBus for Mmu<C> {
    fn read(&self, addr: u16) -> u8 {
        Mmu::read(self, addr)
    }
    fn write(&mut self, addr: u16, val: u8) {
        Mmu::write(self, addr, val)
    }
    fn if_(&self) -> u8 {
        self.if_
    }
    fn set_if(&mut self, val: u8) {
        self.if_ = val;
    }
    fn ie(&self) -> u8 {
        self.ie
    }
}
