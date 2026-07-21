/// Game Boy APU (Audio Processing Unit)
///
/// 4チャンネル構成:
///   CH1: 矩形波 + Sweep (0xFF10-0xFF14)
///   CH2: 矩形波       (0xFF16-0xFF19)
///   CH3: Wave RAM     (0xFF1A-0xFF1E, 0xFF30-0xFF3F)
///   CH4: ノイズ(LFSR) (0xFF20-0xFF23)
///
/// マスター制御: NR50(0xFF24), NR51(0xFF25), NR52(0xFF26)
/// Frame Sequencer: 512 Hz（2048 M-cycle ごと）

// デューティ波形テーブル (CH1/CH2)
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 1, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

// サンプリング: CPU 4.194304 MHz / 4 = 1,048,576 M-cycles/sec
// 44100 Hz → 1サンプル ≒ 23.78 M-cycles
// 分数カウンタ: sample_frac += 44100; if >= 1_048_576 { generate }
const CPU_M_CYCLES_PER_SEC: u32 = 1_048_576;
const SAMPLE_RATE: u32 = 44100;

// Frame Sequencer: 512 Hz = 2048 M-cycles ごとに1ステップ
const FS_PERIOD: u32 = 2048;

// ─── 共通サブ構造体 ───────────────────────────────────────────

struct LengthCounter {
    enabled: bool,
    counter: u16, // CH3は最大256、他は最大64
}

impl LengthCounter {
    fn new(max: u16) -> Self {
        Self { enabled: false, counter: max }
    }

    /// Length Counter をクロック。true を返したらチャンネルを無効化すること。
    fn clock(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            if self.counter == 0 {
                return true;
            }
        }
        false
    }
}

struct VolumeEnvelope {
    initial_vol: u8,
    current_vol: u8,
    add: bool, // true = 音量増加
    pace: u8,
    timer: u8,
}

impl VolumeEnvelope {
    fn new() -> Self {
        Self { initial_vol: 0, current_vol: 0, add: false, pace: 0, timer: 0 }
    }

    fn reload(&mut self) {
        self.current_vol = self.initial_vol;
        self.timer = if self.pace == 0 { 8 } else { self.pace };
    }

    /// Envelope をクロック（Frame Sequencer Step 7 で呼ぶ）
    fn clock(&mut self) {
        if self.pace == 0 {
            return;
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer == 0 {
            self.timer = self.pace;
            if self.add && self.current_vol < 15 {
                self.current_vol += 1;
            } else if !self.add && self.current_vol > 0 {
                self.current_vol -= 1;
            }
        }
    }

    /// DAC が有効かどうか（envelope initial_vol > 0、または add=true）
    fn dac_enabled(&self) -> bool {
        self.initial_vol > 0 || self.add
    }
}

// ─── Channel 1: 矩形波 + Sweep ────────────────────────────────

struct Channel1 {
    // レジスタ
    nr10: u8, // Sweep
    nr11: u8, // Duty / Length load
    nr12: u8, // Envelope
    nr13: u8, // Freq low (write-only)
    nr14: u8, // Freq high + trigger + length enable

    // 内部状態
    enabled: bool,
    duty_pos: u8,
    freq_timer: u16,

    // サブコンポーネント
    length: LengthCounter,
    envelope: VolumeEnvelope,

    // Sweep
    sweep_timer: u8,
    sweep_enabled: bool,
    sweep_shadow: u16, // 周波数シャドウレジスタ
}

impl Channel1 {
    fn new() -> Self {
        Self {
            nr10: 0,
            nr11: 0,
            nr12: 0,
            nr13: 0,
            nr14: 0,
            enabled: false,
            duty_pos: 0,
            freq_timer: 0,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
            sweep_timer: 0,
            sweep_enabled: false,
            sweep_shadow: 0,
        }
    }

    fn freq_val(&self) -> u16 {
        self.nr13 as u16 | ((self.nr14 as u16 & 0x07) << 8)
    }

    fn set_freq(&mut self, freq: u16) {
        self.nr13 = freq as u8;
        self.nr14 = (self.nr14 & 0xF8) | ((freq >> 8) as u8 & 0x07);
    }

    /// M-cycle ごとに周波数タイマーを進める
    fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = (2048 - self.freq_val()) as u16;
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
    }

    /// Trigger (NR14 bit7 書き込み)
    fn trigger(&mut self) {
        self.enabled = self.envelope.dac_enabled();
        self.freq_timer = (2048 - self.freq_val()) as u16;
        if self.length.counter == 0 {
            self.length.counter = 64;
        }
        self.envelope.reload();
        // Sweep 初期化
        self.sweep_shadow = self.freq_val();
        let sweep_pace = (self.nr10 >> 4) & 0x07;
        let sweep_step = self.nr10 & 0x07;
        self.sweep_timer = if sweep_pace == 0 { 8 } else { sweep_pace };
        self.sweep_enabled = sweep_pace > 0 || sweep_step > 0;
        // Sweep step != 0 なら即座にオーバーフローチェック
        if sweep_step > 0 {
            self.sweep_calc_and_check();
        }
    }

    /// Sweep 計算。オーバーフロー（> 2047）なら false を返す。
    fn sweep_calc_and_check(&mut self) -> bool {
        let step = self.nr10 & 0x07;
        let negate = self.nr10 & 0x08 != 0;
        let delta = self.sweep_shadow >> step;
        let new_freq = if negate {
            self.sweep_shadow.wrapping_sub(delta)
        } else {
            self.sweep_shadow + delta
        };
        if new_freq > 2047 {
            self.enabled = false;
            return false;
        }
        true
    }

    /// Sweep クロック（Frame Sequencer Step 2, 6 で呼ぶ）
    fn clock_sweep(&mut self) {
        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }
        if self.sweep_timer == 0 {
            let sweep_pace = (self.nr10 >> 4) & 0x07;
            self.sweep_timer = if sweep_pace == 0 { 8 } else { sweep_pace };
            if self.sweep_enabled && sweep_pace > 0 {
                let step = self.nr10 & 0x07;
                let negate = self.nr10 & 0x08 != 0;
                let delta = self.sweep_shadow >> step;
                let new_freq = if negate {
                    self.sweep_shadow.wrapping_sub(delta)
                } else {
                    self.sweep_shadow + delta
                };
                if new_freq > 2047 {
                    self.enabled = false;
                    return;
                }
                if step > 0 {
                    self.sweep_shadow = new_freq;
                    self.set_freq(new_freq);
                    // 再オーバーフローチェック
                    self.sweep_calc_and_check();
                }
            }
        }
    }

    /// Length カウンターのクロック（Frame Sequencer Step 0,2,4,6 で呼ぶ）
    fn clock_length(&mut self) {
        if self.length.clock() {
            self.enabled = false;
        }
    }

    /// 現在の出力サンプル (0.0 ~ 1.0)
    fn output(&self) -> f32 {
        if !self.enabled || !self.envelope.dac_enabled() {
            return 0.0;
        }
        let duty = (self.nr11 >> 6) as usize;
        let bit = DUTY_TABLE[duty][self.duty_pos as usize];
        bit as f32 * self.envelope.current_vol as f32 / 15.0
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF10 => self.nr10 = val & 0x7F,
            0xFF11 => {
                self.nr11 = val;
                self.length.counter = 64 - (val & 0x3F) as u16;
            }
            0xFF12 => {
                self.nr12 = val;
                self.envelope.initial_vol = val >> 4;
                self.envelope.add = val & 0x08 != 0;
                self.envelope.pace = val & 0x07;
                if !self.envelope.dac_enabled() {
                    self.enabled = false;
                }
            }
            0xFF13 => self.nr13 = val,
            0xFF14 => {
                self.nr14 = val;
                self.length.enabled = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => self.nr10 | 0x80,
            0xFF11 => self.nr11 | 0x3F,
            0xFF12 => self.nr12,
            0xFF13 => 0xFF, // 書き込み専用
            0xFF14 => self.nr14 | 0xBF,
            _ => 0xFF,
        }
    }

    fn reset(&mut self) {
        self.nr10 = 0; self.nr11 = 0; self.nr12 = 0; self.nr13 = 0; self.nr14 = 0;
        self.enabled = false;
        self.duty_pos = 0; self.freq_timer = 0;
        self.length = LengthCounter::new(64);
        self.envelope = VolumeEnvelope::new();
        self.sweep_timer = 0; self.sweep_enabled = false; self.sweep_shadow = 0;
    }
}

// ─── Channel 2: 矩形波（Sweep なし）─────────────────────────────

struct Channel2 {
    nr21: u8,
    nr22: u8,
    nr23: u8,
    nr24: u8,
    enabled: bool,
    duty_pos: u8,
    freq_timer: u16,
    length: LengthCounter,
    envelope: VolumeEnvelope,
}

impl Channel2 {
    fn new() -> Self {
        Self {
            nr21: 0, nr22: 0, nr23: 0, nr24: 0,
            enabled: false, duty_pos: 0, freq_timer: 0,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
        }
    }

    fn freq_val(&self) -> u16 {
        self.nr23 as u16 | ((self.nr24 as u16 & 0x07) << 8)
    }

    fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = (2048 - self.freq_val()) as u16;
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.envelope.dac_enabled();
        self.freq_timer = (2048 - self.freq_val()) as u16;
        if self.length.counter == 0 {
            self.length.counter = 64;
        }
        self.envelope.reload();
    }

    fn clock_length(&mut self) {
        if self.length.clock() {
            self.enabled = false;
        }
    }

    fn output(&self) -> f32 {
        if !self.enabled || !self.envelope.dac_enabled() {
            return 0.0;
        }
        let duty = (self.nr21 >> 6) as usize;
        let bit = DUTY_TABLE[duty][self.duty_pos as usize];
        bit as f32 * self.envelope.current_vol as f32 / 15.0
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF16 => {
                self.nr21 = val;
                self.length.counter = 64 - (val & 0x3F) as u16;
            }
            0xFF17 => {
                self.nr22 = val;
                self.envelope.initial_vol = val >> 4;
                self.envelope.add = val & 0x08 != 0;
                self.envelope.pace = val & 0x07;
                if !self.envelope.dac_enabled() {
                    self.enabled = false;
                }
            }
            0xFF18 => self.nr23 = val,
            0xFF19 => {
                self.nr24 = val;
                self.length.enabled = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF16 => self.nr21 | 0x3F,
            0xFF17 => self.nr22,
            0xFF18 => 0xFF,
            0xFF19 => self.nr24 | 0xBF,
            _ => 0xFF,
        }
    }

    fn reset(&mut self) {
        self.nr21 = 0; self.nr22 = 0; self.nr23 = 0; self.nr24 = 0;
        self.enabled = false; self.duty_pos = 0; self.freq_timer = 0;
        self.length = LengthCounter::new(64);
        self.envelope = VolumeEnvelope::new();
    }
}

// ─── Channel 3: Wave RAM ─────────────────────────────────────

struct Channel3 {
    nr30: u8,
    nr31: u8,
    nr32: u8,
    nr33: u8,
    nr34: u8,
    enabled: bool,
    wave_pos: u8, // 0-31
    freq_timer: u16,
    length: LengthCounter,
    wave_ram: [u8; 16], // 0xFF30-0xFF3F: 32個の4ビットサンプル
}

impl Channel3 {
    fn new() -> Self {
        Self {
            nr30: 0, nr31: 0, nr32: 0, nr33: 0, nr34: 0,
            enabled: false, wave_pos: 0, freq_timer: 0,
            length: LengthCounter::new(256),
            wave_ram: [0; 16],
        }
    }

    fn freq_val(&self) -> u16 {
        self.nr33 as u16 | ((self.nr34 as u16 & 0x07) << 8)
    }

    fn dac_enabled(&self) -> bool {
        self.nr30 & 0x80 != 0
    }

    fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            // CH3のリロード値は (2048 - freq_val) * 2 T-cycles = (2048 - freq_val) / 2 M-cycles
            // ただし sdl2 は T-cycle 単位ではなく M-cycle 単位で動くため
            // 実際には (2048 - freq_val) を使い、tick()が 1 M-cycle ごとに呼ばれる
            self.freq_timer = 2048 - self.freq_val();
            self.wave_pos = (self.wave_pos + 1) & 31;
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled();
        self.freq_timer = 2048 - self.freq_val();
        self.wave_pos = 0;
        if self.length.counter == 0 {
            self.length.counter = 256;
        }
    }

    fn clock_length(&mut self) {
        if self.length.clock() {
            self.enabled = false;
        }
    }

    fn current_sample(&self) -> u8 {
        let byte = self.wave_ram[self.wave_pos as usize / 2];
        if self.wave_pos % 2 == 0 { byte >> 4 } else { byte & 0x0F }
    }

    fn output(&self) -> f32 {
        if !self.enabled || !self.dac_enabled() {
            return 0.0;
        }
        let sample = self.current_sample();
        // NR32 bits 6-5: 出力レベル
        let shifted = match (self.nr32 >> 5) & 0x03 {
            0 => 0,
            1 => sample,
            2 => sample >> 1,
            _ => sample >> 2,
        };
        shifted as f32 / 15.0
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF1A => {
                self.nr30 = val & 0x80;
                if !self.dac_enabled() {
                    self.enabled = false;
                }
            }
            0xFF1B => {
                self.nr31 = val;
                self.length.counter = 256 - val as u16;
            }
            0xFF1C => self.nr32 = val & 0x60,
            0xFF1D => self.nr33 = val,
            0xFF1E => {
                self.nr34 = val;
                self.length.enabled = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger();
                }
            }
            0xFF30..=0xFF3F => {
                // CH3有効時: wave_pos/2 のバイトのみアクセス可（実機挙動）
                if self.enabled {
                    self.wave_ram[self.wave_pos as usize / 2] = val;
                } else {
                    self.wave_ram[(addr - 0xFF30) as usize] = val;
                }
            }
            _ => {}
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF1A => self.nr30 | 0x7F,
            0xFF1B => 0xFF, // 書き込み専用
            0xFF1C => self.nr32 | 0x9F,
            0xFF1D => 0xFF,
            0xFF1E => self.nr34 | 0xBF,
            0xFF30..=0xFF3F => {
                if self.enabled {
                    self.wave_ram[self.wave_pos as usize / 2]
                } else {
                    self.wave_ram[(addr - 0xFF30) as usize]
                }
            }
            _ => 0xFF,
        }
    }

    fn reset(&mut self) {
        self.nr30 = 0; self.nr31 = 0; self.nr32 = 0; self.nr33 = 0; self.nr34 = 0;
        self.enabled = false; self.wave_pos = 0; self.freq_timer = 0;
        self.length = LengthCounter::new(256);
        // Wave RAM はリセットしない（電源 OFF でも保持）
    }
}

// ─── Channel 4: ノイズ (LFSR) ────────────────────────────────

struct Channel4 {
    nr41: u8,
    nr42: u8,
    nr43: u8,
    nr44: u8,
    enabled: bool,
    lfsr: u16, // 15ビット
    freq_timer: u32,
    length: LengthCounter,
    envelope: VolumeEnvelope,
}

impl Channel4 {
    fn new() -> Self {
        Self {
            nr41: 0, nr42: 0, nr43: 0, nr44: 0,
            enabled: false, lfsr: 0x7FFF, freq_timer: 0,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
        }
    }

    fn timer_period(&self) -> u32 {
        let divisor_code = self.nr43 & 0x07;
        let clock_shift = (self.nr43 >> 4) as u32;
        let divisor: u32 = if divisor_code == 0 { 8 } else { divisor_code as u32 * 16 };
        divisor << clock_shift
    }

    fn tick(&mut self) {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }
        if self.freq_timer == 0 {
            self.freq_timer = self.timer_period();
            // LFSR: XOR bit0 と bit1
            let xor_bit = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr >>= 1;
            self.lfsr |= xor_bit << 14;
            // 7ビットモード
            if self.nr43 & 0x08 != 0 {
                self.lfsr = (self.lfsr & !0x40) | (xor_bit << 6);
            }
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.envelope.dac_enabled();
        self.lfsr = 0x7FFF;
        self.freq_timer = self.timer_period();
        if self.length.counter == 0 {
            self.length.counter = 64;
        }
        self.envelope.reload();
    }

    fn clock_length(&mut self) {
        if self.length.clock() {
            self.enabled = false;
        }
    }

    fn output(&self) -> f32 {
        if !self.enabled || !self.envelope.dac_enabled() {
            return 0.0;
        }
        // LFSR bit0 が 0 のとき音が出る
        if self.lfsr & 1 == 0 {
            self.envelope.current_vol as f32 / 15.0
        } else {
            0.0
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF20 => {
                self.nr41 = val & 0x3F;
                self.length.counter = 64 - (val & 0x3F) as u16;
            }
            0xFF21 => {
                self.nr42 = val;
                self.envelope.initial_vol = val >> 4;
                self.envelope.add = val & 0x08 != 0;
                self.envelope.pace = val & 0x07;
                if !self.envelope.dac_enabled() {
                    self.enabled = false;
                }
            }
            0xFF22 => self.nr43 = val,
            0xFF23 => {
                self.nr44 = val;
                self.length.enabled = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF20 => 0xFF,
            0xFF21 => self.nr42,
            0xFF22 => self.nr43,
            0xFF23 => self.nr44 | 0xBF,
            _ => 0xFF,
        }
    }

    fn reset(&mut self) {
        self.nr41 = 0; self.nr42 = 0; self.nr43 = 0; self.nr44 = 0;
        self.enabled = false; self.lfsr = 0x7FFF; self.freq_timer = 0;
        self.length = LengthCounter::new(64);
        self.envelope = VolumeEnvelope::new();
    }
}

// ─── APU メイン構造体 ─────────────────────────────────────────

pub struct Apu {
    ch1: Channel1,
    ch2: Channel2,
    ch3: Channel3,
    ch4: Channel4,

    nr50: u8, // マスターボリューム / VIN (0xFF24)
    nr51: u8, // チャンネルパンニング (0xFF25)

    powered: bool, // NR52 bit7

    // Frame Sequencer
    fs_counter: u32,
    fs_step: u8,

    // サンプリング（分数カウンタ方式）
    sample_frac: u32,

    // ダウンサンプリング用のボックスフィルタ累積 (前回サンプル以降の mix() 出力の総和)。
    // 瞬時値の間引きだと矩形波/ノイズの高調波が折り返してエイリアシング (耳障りな
    // ざらつき) になるため、サンプル間の全 M-cycle を平均してから出力する。
    acc_l: f32,
    acc_r: f32,
    acc_n: u32,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            ch1: Channel1::new(),
            ch2: Channel2::new(),
            ch3: Channel3::new(),
            ch4: Channel4::new(),
            nr50: 0,
            nr51: 0,
            powered: false,
            fs_counter: 0,
            fs_step: 0,
            sample_frac: 0,
            acc_l: 0.0,
            acc_r: 0.0,
            acc_n: 0,
        }
    }

    /// 1 M-cycle 進める。サンプリングタイミングなら `Some((left, right))` を返す。
    pub fn emulate_cycle(&mut self) -> Option<(f32, f32)> {
        if !self.powered {
            // 無音サンプルでカウンタを回す
            self.acc_l = 0.0;
            self.acc_r = 0.0;
            self.acc_n = 0;
            self.sample_frac += SAMPLE_RATE;
            if self.sample_frac >= CPU_M_CYCLES_PER_SEC {
                self.sample_frac -= CPU_M_CYCLES_PER_SEC;
                return Some((0.0, 0.0));
            }
            return None;
        }

        // 1. Frame Sequencer クロック
        self.fs_counter += 1;
        if self.fs_counter >= FS_PERIOD {
            self.fs_counter = 0;
            self.clock_frame_sequencer();
        }

        // 2. 各チャンネルの周波数タイマーを進める
        self.ch1.tick();
        self.ch2.tick();
        self.ch3.tick();
        self.ch4.tick();

        // 3. ミキサー出力を毎サイクル積算 (ボックスフィルタ。struct のコメント参照)
        let (l, r) = self.mix();
        self.acc_l += l;
        self.acc_r += r;
        self.acc_n += 1;

        // 4. サンプリング判定: 前回サンプル以降の平均を出力する
        self.sample_frac += SAMPLE_RATE;
        if self.sample_frac >= CPU_M_CYCLES_PER_SEC {
            self.sample_frac -= CPU_M_CYCLES_PER_SEC;
            let n = self.acc_n as f32;
            let out = (self.acc_l / n, self.acc_r / n);
            self.acc_l = 0.0;
            self.acc_r = 0.0;
            self.acc_n = 0;
            return Some(out);
        }
        None
    }

    fn clock_frame_sequencer(&mut self) {
        match self.fs_step {
            0 | 4 => {
                // Length カウンター
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
                self.ch4.clock_length();
            }
            2 | 6 => {
                // Length カウンター + Sweep
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
                self.ch4.clock_length();
                self.ch1.clock_sweep();
            }
            7 => {
                // Envelope
                self.ch1.envelope.clock();
                self.ch2.envelope.clock();
                self.ch4.envelope.clock();
            }
            _ => {}
        }
        self.fs_step = (self.fs_step + 1) & 7;
    }

    fn mix(&self) -> (f32, f32) {
        let ch1 = self.ch1.output();
        let ch2 = self.ch2.output();
        let ch3 = self.ch3.output();
        let ch4 = self.ch4.output();

        // NR51: bit7=CH4左, bit6=CH3左, bit5=CH2左, bit4=CH1左
        //       bit3=CH4右, bit2=CH3右, bit1=CH2右, bit0=CH1右
        let left = (if self.nr51 & 0x10 != 0 { ch1 } else { 0.0 })
            + (if self.nr51 & 0x20 != 0 { ch2 } else { 0.0 })
            + (if self.nr51 & 0x40 != 0 { ch3 } else { 0.0 })
            + (if self.nr51 & 0x80 != 0 { ch4 } else { 0.0 });
        let right = (if self.nr51 & 0x01 != 0 { ch1 } else { 0.0 })
            + (if self.nr51 & 0x02 != 0 { ch2 } else { 0.0 })
            + (if self.nr51 & 0x04 != 0 { ch3 } else { 0.0 })
            + (if self.nr51 & 0x08 != 0 { ch4 } else { 0.0 });

        // NR50: bits 6-4 = 左ボリューム(0-7), bits 2-0 = 右ボリューム(0-7)
        let left_vol = ((self.nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_vol = (self.nr50 & 0x07) as f32 / 7.0;

        // 4チャンネル合計(最大4.0)を正規化 → ボリューム適用 → 0.5 で余裕を持たせる
        let l = (left / 4.0) * left_vol * 0.5;
        let r = (right / 4.0) * right_vol * 0.5;
        (l, r)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // CH1
            0xFF10 => self.ch1.read(addr),
            0xFF11 => self.ch1.read(addr),
            0xFF12 => self.ch1.read(addr),
            0xFF13 => 0xFF,
            0xFF14 => self.ch1.read(addr),
            // CH2
            0xFF15 => 0xFF, // 未使用
            0xFF16 => self.ch2.read(addr),
            0xFF17 => self.ch2.read(addr),
            0xFF18 => 0xFF,
            0xFF19 => self.ch2.read(addr),
            // CH3
            0xFF1A => self.ch3.read(addr),
            0xFF1B => 0xFF,
            0xFF1C => self.ch3.read(addr),
            0xFF1D => 0xFF,
            0xFF1E => self.ch3.read(addr),
            // CH4
            0xFF1F => 0xFF, // 未使用
            0xFF20 => 0xFF,
            0xFF21 => self.ch4.read(addr),
            0xFF22 => self.ch4.read(addr),
            0xFF23 => self.ch4.read(addr),
            // マスター
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => {
                let powered_bit = if self.powered { 0x80 } else { 0x00 };
                let ch1_bit = if self.ch1.enabled { 0x01 } else { 0x00 };
                let ch2_bit = if self.ch2.enabled { 0x02 } else { 0x00 };
                let ch3_bit = if self.ch3.enabled { 0x04 } else { 0x00 };
                let ch4_bit = if self.ch4.enabled { 0x08 } else { 0x00 };
                powered_bit | ch1_bit | ch2_bit | ch3_bit | ch4_bit | 0x70
            }
            // Wave RAM
            0xFF30..=0xFF3F => self.ch3.read(addr),
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        // NR52 と Wave RAM は電源 OFF でも書き込み可
        match addr {
            0xFF26 => {
                let was_powered = self.powered;
                self.powered = val & 0x80 != 0;
                if was_powered && !self.powered {
                    // 電源 OFF: 0xFF10-0xFF25 をリセット
                    self.ch1.reset();
                    self.ch2.reset();
                    self.ch3.reset();
                    self.ch4.reset();
                    self.nr50 = 0;
                    self.nr51 = 0;
                } else if !was_powered && self.powered {
                    // 電源 ON: Frame Sequencer リセット
                    self.fs_step = 0;
                    self.fs_counter = 0;
                }
                return;
            }
            0xFF30..=0xFF3F => {
                self.ch3.write(addr, val);
                return;
            }
            _ => {}
        }

        // 電源 OFF 中は NR52/Wave RAM 以外を無視
        if !self.powered {
            return;
        }

        match addr {
            0xFF10..=0xFF14 => self.ch1.write(addr, val),
            0xFF15 => {}
            0xFF16..=0xFF19 => self.ch2.write(addr, val),
            0xFF1A..=0xFF1E => self.ch3.write(addr, val),
            0xFF1F => {}
            0xFF20..=0xFF23 => self.ch4.write(addr, val),
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            _ => {}
        }
    }
}
