/// I2S オーディオ出力 スタブ (PCM5102A または SGTL5000 向け)。
///
/// # ピン割り当て (Teensy 4.1, SAI1 TX)
///
/// | 信号          | Teensy ピン | パッド         | 備考               |
/// |--------------|------------|---------------|--------------------|
/// | SAI1_TX_DATA | 7          | GPIO_B1_01    | PCM5102 DIN        |
/// | SAI1_TX_BCLK | 26         | GPIO_AD_B1_14 | PCM5102 BCK        |
/// | SAI1_TX_SYNC | 27         | GPIO_AD_B1_15 | PCM5102 LRCK       |
///
/// NOTE: pin 7 を SAI に使うため BL (バックライト) は pin 6 に変更すること。
///
/// # SAI/I2S クロック設定
///
/// 44.1 kHz, 16-bit, stereo の場合:
///   BCLK = 44100 × 2 ch × 16 bit = 1,411,200 Hz
///   Audio PLL (PLL4) → 11.2896 MHz → bclk_div(8) → BCLK = 1.4112 MHz ✓
///
/// 実装手順 (TODO):
///   1. `ccm_analog` で Audio PLL を 11.2896 MHz に設定
///   2. CCM SAI1 clock root に Audio PLL を割り当て
///   3. `hal::sai::Sai::new()` + `SaiConfig::i2s(bclk_div(8))` で TX 初期化
///   4. `sai_tx.set_enable(true)` で有効化
///   5. `push()` で f32 サンプルを i16 に変換し `sai_tx.write_frame(0, [l, r])` で送出
///
/// 参考実装: imxrt-hal/examples/rtic_sai_pcm5102.rs
use gb_core::platform::AudioSink;

/// PCM5102A I2S オーディオシンク (スタブ)。
///
/// 現在は no-op。SAI 初期化を実装したら `sai_tx` を保持するように変更する。
pub struct PcmAudio;

#[allow(dead_code)]
impl PcmAudio {
    pub fn new() -> Self {
        // TODO: SAI1 初期化
        //
        // ```rust
        // // 1. Audio PLL を 11.2896 MHz に設定 (44.1 kHz 系)
        // // ccm_analog: PLL4 設定 (imxrt-hal ccm::sai_clk 参照)
        //
        // // 2. SAI 初期化
        // // pin 7   → SAI1_TX_DATA00 として IOMUXC 設定
        // // pin 26  → SAI1_TX_BCLK   として IOMUXC 設定
        // // pin 27  → SAI1_TX_SYNC   として IOMUXC 設定
        // let sai = bsp::hal::sai::Sai::new(sai1_ral, tx_pins, rx_pins);
        // let mut cfg = bsp::hal::sai::SaiConfig::i2s(bsp::hal::sai::bclk_div(8));
        // let (Some(tx), None) = sai.split(&cfg) else { panic!() };
        // tx.set_enable(true);
        // ```
        Self
    }
}

impl AudioSink for PcmAudio {
    fn push(&mut self, _left: f32, _right: f32) {
        // TODO: SAI TX FIFO へ書き込む
        //
        // ```rust
        // // f32 (-1.0..1.0) → i16 変換
        // let l = (_left.clamp(-1.0, 1.0) * i16::MAX as f32) as i16 as u16;
        // let r = (_right.clamp(-1.0, 1.0) * i16::MAX as f32) as i16 as u16;
        // // FIFO に空きがあれば書き込む (フル時は破棄)
        // let status = self.sai_tx.status();
        // if !status.contains(bsp::hal::sai::Status::FIFO_ERROR) {
        //     self.sai_tx.write_frame(0, [l, r]);
        // }
        // ```
    }
}
