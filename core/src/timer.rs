pub struct Timer {
    div: u8,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    div_counter: u32,
    tima_counter: u32,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            div_counter: 0,
            tima_counter: 0,
        }
    }

    /// 1 M-cycle進める。タイマー割り込みが発生したら true を返す。
    pub fn emulate_cycle(&mut self) -> bool {
        self.div_counter = self.div_counter.wrapping_add(1);
        // DIV は 64 M-cycle ごとにインクリメント (4MHz / 64 = 16384Hz)
        if self.div_counter % 64 == 0 {
            self.div = self.div.wrapping_add(1);
        }

        if self.tac & 0x04 == 0 {
            return false;
        }

        self.tima_counter = self.tima_counter.wrapping_add(1);
        let threshold: u32 = match self.tac & 0x03 {
            0 => 256, // 4096 Hz
            1 => 4,   // 262144 Hz
            2 => 16,  // 65536 Hz
            3 => 64,  // 16384 Hz
            _ => unreachable!(),
        };

        if self.tima_counter >= threshold {
            self.tima_counter = 0;
            let (new_tima, overflow) = self.tima.overflowing_add(1);
            if overflow {
                self.tima = self.tma;
                return true;
            }
            self.tima = new_tima;
        }
        false
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF04 => {
                // DIV への書き込みはリセット
                self.div = 0;
                self.div_counter = 0;
            }
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => self.tac = val & 0x07,
            _ => {}
        }
    }
}
