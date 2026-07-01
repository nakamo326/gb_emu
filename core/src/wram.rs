/// CGB: 32KB WRAM（バンク 0 固定 0xC000–0xCFFF + バンク 1–7 を SVBK で 0xD000–0xDFFF に切替）
/// DMG: 8KB 相当（バンク 0 + バンク 1 固定）
pub struct WRam {
    banks: [[u8; 0x1000]; 8],
    /// 現在選択中のバンク番号（1–7、0 を書いたら 1 として扱う）
    svbk: u8,
}

impl WRam {
    pub fn new() -> Self {
        Self { banks: [[0; 0x1000]; 8], svbk: 1 }
    }

    /// SVBK (0xFF70) 読み取り
    pub fn read_svbk(&self) -> u8 {
        // 上位 5bit は 1 固定
        self.svbk | 0xF8
    }

    /// SVBK (0xFF70) 書き込み
    pub fn write_svbk(&mut self, val: u8) {
        let n = val & 0x07;
        self.svbk = if n == 0 { 1 } else { n };
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xC000..=0xCFFF => self.banks[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.banks[self.svbk as usize][(addr - 0xD000) as usize],
            // エコー領域 0xE000–0xFDFF は MMU 側でマスクして渡すが念のため対応
            0xE000..=0xEFFF => self.banks[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.banks[self.svbk as usize][(addr - 0xF000) as usize],
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xC000..=0xCFFF => self.banks[0][(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => self.banks[self.svbk as usize][(addr - 0xD000) as usize] = val,
            0xE000..=0xEFFF => self.banks[0][(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => self.banks[self.svbk as usize][(addr - 0xF000) as usize] = val,
            _ => {}
        }
    }
}
