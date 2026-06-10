/// 実 GB ROM カートリッジ GPIO バスドライバ。
///
/// # ピン割り当て (Teensy 4.1, 要実機確認)
///
/// | 信号    | Teensy ピン | GPIO ポート / ビット | 備考             |
/// |--------|------------|-------------------|-----------------|
/// | D0     | 14         | GPIO1[18]         | GPIO_AD_B1_02   |
/// | D1     | 15         | GPIO1[19]         | GPIO_AD_B1_03   |
/// | D2     | 40         | GPIO1[20]         | GPIO_AD_B1_04   |
/// | D3     | 41         | GPIO1[21]         | GPIO_AD_B1_05   |
/// | D4     | 17         | GPIO1[22]         | GPIO_AD_B1_06   |
/// | D5     | 16         | GPIO1[23]         | GPIO_AD_B1_07   |
/// | D6     | 22         | GPIO1[24]         | GPIO_AD_B1_08   |
/// | D7     | 23         | GPIO1[25]         | GPIO_AD_B1_09   |
/// | A0–A7  | 19,18,38,39,26,27,0,1 | GPIO1[16-17,28-31,2-3] ※ 非連続 |
/// | A8–A15 | 拡張パッド  | GPIO3/4 (TODO)    |                 |
/// | /RD    | 33         | GPIO4[7]          | GPIO_EMC_07     |
/// | /WR    | 34         | GPIO2[28]         | GPIO_B1_12 (t41)|
///
/// # 注意
///
/// - GB カートリッジは 5V 系（DMG）または 3.3V 系（GBC）。
///   Teensy 4.1 は 3.3V のため、5V DMG カートリッジには 74AHCT245 等が必要。
/// - IOMUXC は事前に `gpio_port.output(pin)` / `.input(pin)` で GPIO モードに設定すること。
use gb_core::platform::CartridgeBus;
use teensy4_bsp as bsp;
use bsp::hal::gpio::{Input, Output, Port};
use cortex_m::asm;

/// D0-D7 は GPIO1 ビット 18-25 に連続配置
const DATA_SHIFT: u32 = 18;
const DATA_MASK: u32 = 0xFF << DATA_SHIFT;

// TODO: A0-A15 のビットマスクは実際のピン配置に応じて調整すること
// 現在は GPIO1 ビット 0-15 を仮定した直接マッピング（要変更）
const ADDR_MASK: u32 = 0x0000_FFFF;

/// /RD = GPIO4 bit 7 (pin 33 = GPIO_EMC_07)
const N_RD_OFFSET: u32 = 7;
/// /WR = GPIO2 bit 28 (pin 34 t41 = GPIO_B1_12 → GPIO2[28])
const N_WR_OFFSET: u32 = 28;

/// ROM アクセスタイム待ち ≥ 150 ns @ 600 MHz ≈ 90 cycles
const ACCESS_DELAY: u32 = 90;

pub struct GpioCart {
    /// データバスの方向切り替えに使用する Port<1>
    data_port: Port<1>,
    /// /RD 制御 (GPIO4, &'static RegisterBlock を保持)
    n_rd: Output<()>,
    /// /WR 制御 (GPIO2, &'static RegisterBlock を保持)
    n_wr: Output<()>,
}

impl GpioCart {
    /// GPIO カートリッジバスを構築する。
    ///
    /// # Safety
    ///
    /// 呼び出し前に以下を保証すること:
    /// 1. `board::t41()` によるクロックゲート・電源設定の完了
    /// 2. データバスピン (D0-D7)、アドレスバスピン (A0-A15)、制御ピン (/RD, /WR) に対して
    ///    `gpio_port.output(pin)` を呼び IOMUXC を GPIO モードに設定
    /// 3. `gpio4` および `gpio2` は本関数への移動後に別のコードから使用しないこと
    pub unsafe fn new(
        mut data_port: Port<1>,
        mut gpio4: Port<4>,
        mut gpio2: Port<2>,
    ) -> Self {
        // アドレスバス A0-A15 を出力に設定 (GPIO1 bit 0-15)
        for bit in 0..16u32 {
            let _ = Output::<()>::without_pin(&mut data_port, bit);
        }

        // データバス D0-D7 を入力に設定 (デフォルト状態)
        for bit in DATA_SHIFT..DATA_SHIFT + 8 {
            let _ = Input::<()>::without_pin(&mut data_port, bit);
        }

        // /RD = 出力・非アサート (HIGH)
        let n_rd = Output::<()>::without_pin(&mut gpio4, N_RD_OFFSET);
        n_rd.set();

        // /WR = 出力・非アサート (HIGH)
        let n_wr = Output::<()>::without_pin(&mut gpio2, N_WR_OFFSET);
        n_wr.set();

        // gpio4, gpio2 はここで drop。Output<()> は &'static RegisterBlock を保持するため
        // Port が drop された後も使用可能（ハードウェアアドレスは常に有効）。
        Self { data_port, n_rd, n_wr }
    }
}

impl CartridgeBus for GpioCart {
    fn read(&self, addr: u16) -> u8 {
        // Safety: single-threaded embedded, GPIO1 クロックゲートは board::t41() で有効化済み
        let gpio1 = unsafe { bsp::ral::gpio::GPIO1::instance() };

        // アドレス出力
        bsp::ral::modify_reg!(bsp::ral::gpio, gpio1, DR,
            |dr| (dr & !ADDR_MASK) | (addr as u32 & ADDR_MASK));

        // /RD をアサート → 待機 → データ読み取り → デアサート
        self.n_rd.clear();
        asm::delay(ACCESS_DELAY);
        let val = ((bsp::ral::read_reg!(bsp::ral::gpio, gpio1, PSR) >> DATA_SHIFT) & 0xFF) as u8;
        self.n_rd.set();
        val
    }

    fn write(&mut self, addr: u16, val: u8) {
        let gpio1 = unsafe { bsp::ral::gpio::GPIO1::instance() };

        // アドレス出力
        bsp::ral::modify_reg!(bsp::ral::gpio, gpio1, DR,
            |dr| (dr & !ADDR_MASK) | (addr as u32 & ADDR_MASK));

        // データバスを出力に切り替え → 書き込み
        for bit in DATA_SHIFT..DATA_SHIFT + 8 {
            let _ = Output::<()>::without_pin(&mut self.data_port, bit);
        }
        bsp::ral::modify_reg!(bsp::ral::gpio, gpio1, DR,
            |dr| (dr & !DATA_MASK) | ((val as u32) << DATA_SHIFT));

        // /WR アサート → 待機 → デアサート
        self.n_wr.clear();
        asm::delay(ACCESS_DELAY);
        self.n_wr.set();

        // データバスを入力に戻す
        for bit in DATA_SHIFT..DATA_SHIFT + 8 {
            let _ = Input::<()>::without_pin(&mut self.data_port, bit);
        }
    }
}
