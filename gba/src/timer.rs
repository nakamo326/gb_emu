//! タイマー 4 本（プリスケーラ / カスケード / IRQ）。
//! サウンド FIFO 連携は APU 未実装のため持たない。

#[derive(Default, Clone, Copy)]
struct Timer {
    reload: u16,
    counter: u16,
    ctrl: u16,
    /// プリスケーラの端数サイクル
    acc: u32,
}

pub struct Timers {
    t: [Timer; 4],
}

impl Timers {
    pub fn new() -> Self {
        Self { t: [Timer::default(); 4] }
    }

    /// cycles 分進め、発生した IRQ の IF ビット（bit3-6）を返す。
    pub fn step(&mut self, cycles: u32) -> u16 {
        let mut irq = 0u16;
        let mut prev_overflows = 0u32;
        for i in 0..4 {
            let t = &mut self.t[i];
            if t.ctrl & 0x80 == 0 {
                prev_overflows = 0;
                continue;
            }
            let ticks = if t.ctrl & 0x04 != 0 && i > 0 {
                prev_overflows
            } else {
                let shift = match t.ctrl & 3 {
                    0 => 0,
                    1 => 6,
                    2 => 8,
                    _ => 10,
                };
                t.acc += cycles;
                let n = t.acc >> shift;
                t.acc -= n << shift;
                n
            };
            let mut overflows = 0u32;
            if ticks > 0 {
                let space = 0x1_0000 - t.counter as u32;
                if ticks >= space {
                    let period = 0x1_0000 - t.reload as u32;
                    overflows = 1 + (ticks - space) / period;
                    t.counter = (t.reload as u32 + (ticks - space) % period) as u16;
                    if t.ctrl & 0x40 != 0 {
                        irq |= 0x08 << i;
                    }
                } else {
                    t.counter += ticks as u16;
                }
            }
            prev_overflows = overflows;
        }
        irq
    }

    pub fn read_io(&self, off: usize) -> u16 {
        let idx = (off - 0x100) / 4;
        if (off - 0x100) % 4 == 0 { self.t[idx].counter } else { self.t[idx].ctrl }
    }

    pub fn write_io(&mut self, off: usize, val: u16, mask: u16) {
        let idx = (off - 0x100) / 4;
        let t = &mut self.t[idx];
        if (off - 0x100) % 4 == 0 {
            t.reload = t.reload & !mask | val & mask;
        } else {
            let old = t.ctrl;
            t.ctrl = t.ctrl & !mask | val & mask;
            // 有効化の立ち上がりでリロード
            if old & 0x80 == 0 && t.ctrl & 0x80 != 0 {
                t.counter = t.reload;
                t.acc = 0;
            }
        }
    }
}
