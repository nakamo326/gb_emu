//! DMA 4 チャンネル。レジスタ管理とトリガー判定のみを持ち、
//! 実際の転送はバス全体へアクセスできる [`crate::bus::Bus::dma_service`] が行う。

/// サウンド FIFO DMA（タイミング 3）は APU 未実装のため起動しない。

#[derive(Default, Clone, Copy)]
pub struct Channel {
    pub sad: u32,
    pub dad: u32,
    /// CNT_L（転送カウントのリロード値）
    pub count: u16,
    /// CNT_H（制御）
    pub cnt: u16,
    // 有効化時にラッチされる内部状態
    pub src: u32,
    pub dst: u32,
    pub remaining: u32,
    pub pending: bool,
}

pub struct Dma {
    pub ch: [Channel; 4],
}

impl Dma {
    pub fn new() -> Self {
        Self { ch: [Channel::default(); 4] }
    }

    /// カウント 0 は最大値扱い（ch0-2: 0x4000, ch3: 0x10000）
    pub fn count_of(idx: usize, count: u16) -> u32 {
        let max = if idx == 3 { 0x1_0000 } else { 0x4000 };
        let masked = count as u32 & (max - 1);
        if masked == 0 { max } else { masked }
    }

    /// 指定タイミング (1:vblank, 2:hblank) の有効チャンネルを起動する。
    pub fn trigger(&mut self, timing: u16) {
        for ch in &mut self.ch {
            if ch.cnt & 0x8000 != 0 && (ch.cnt >> 12) & 3 == timing {
                ch.pending = true;
            }
        }
    }

    /// 優先度順（ch0 が最優先）で pending のチャンネル番号を返す。
    pub fn next_pending(&self) -> Option<usize> {
        self.ch.iter().position(|c| c.pending)
    }

    pub fn read_io(&self, off: usize) -> u16 {
        let idx = (off - 0xB0) / 12;
        // CNT_H のみ読み出し可。SAD/DAD/CNT_L は書き込み専用
        if (off - 0xB0) % 12 == 10 { self.ch[idx].cnt } else { 0 }
    }

    pub fn write_io(&mut self, off: usize, val: u16, mask: u16) {
        let idx = (off - 0xB0) / 12;
        let ch = &mut self.ch[idx];
        let merge = |cur: u16| cur & !mask | val & mask;
        match (off - 0xB0) % 12 {
            0 => ch.sad = ch.sad & !(mask as u32) | (val & mask) as u32,
            2 => ch.sad = ch.sad & !((mask as u32) << 16) | ((val & mask) as u32) << 16,
            4 => ch.dad = ch.dad & !(mask as u32) | (val & mask) as u32,
            6 => ch.dad = ch.dad & !((mask as u32) << 16) | ((val & mask) as u32) << 16,
            8 => ch.count = merge(ch.count),
            10 => {
                let old = ch.cnt;
                ch.cnt = merge(ch.cnt);
                // 有効化の立ち上がりで内部状態をラッチ
                if old & 0x8000 == 0 && ch.cnt & 0x8000 != 0 {
                    ch.src = ch.sad & 0x0FFF_FFFF;
                    ch.dst = ch.dad & 0x0FFF_FFFF;
                    ch.remaining = Self::count_of(idx, ch.count);
                    if (ch.cnt >> 12) & 3 == 0 {
                        ch.pending = true;
                    }
                }
                if ch.cnt & 0x8000 == 0 {
                    ch.pending = false;
                }
            }
            _ => {}
        }
    }
}
