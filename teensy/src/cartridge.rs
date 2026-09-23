use bsp::hal::gpio::Port;
use bsp::pins::t41::{
    P0, P1, P2, P3, P4, P5, P6, P8, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25,
    P33, P34, P35, P37, P38, P39, P40, P41,
};
use bsp::ral::{self, gpio::{GPIO1, GPIO2, GPIO4}};
use cortex_m::asm;
use gb_core::platform::CartridgeBus;
use teensy4_bsp as bsp;

/// 実 GB カートリッジ GPIO バスドライバ。
///
/// # ピン割り当て (Teensy 4.1, 確定)
///
/// | 信号    | Teensy ピン | GPIO ポート / ビット | 備考 |
/// |--------|------------|-------------------|-----|
/// | D0-D7  | 14,15,40,41,17,16,22,23 | GPIO1[18-25] | 連続。D7(p23) は SAI1_MCLK と共用 (下記) |
/// | A0-A9  | 19,18,38,39,24,25,0,1,20,21 | GPIO1[16,17,28,29,12,13,3,2,26,27] | 非連続 |
/// | A10-A13| 2,3,4,5    | GPIO4[4,5,6,8]    | |
/// | A14    | 6          | GPIO2[10]         | |
/// | A15    | 8          | GPIO2[16]         | ディスプレイ RST から転用 (RST は 3.3V 固定) |
/// | /RD    | 33         | GPIO4[7]          | 74AHCT245 (データバス) の DIR にも直結 |
/// | /WR    | 34         | GPIO2[29]         | GPIO_B1_13 |
/// | /CS    | 35         | GPIO2[28]         | GPIO_B1_12。0xA000-0xBFFF (外部 RAM) でアサート |
/// | /RESET | 37         | GPIO2[19]         | 初期化時に L→H パルスで MBC を初期化 |
///
/// # 配線の注意
///
/// - GB カートリッジは 5V 系。アドレス・制御線は 74AHCT245、データバスは 2 電源の SN74LVC8T245
///   (VCCA=3.3V/VCCB=5V) でレベル変換する (docs/real_cart_wiring.md)。
/// - A15 を省略すると MBC が外部 RAM アクセスを ROM 域 (レジスタ書き込み・ROM 選択) と
///   誤認するため、A15 は必須。
/// - CLK / AUDIO_IN は未接続でよい。
pub struct GpioCart {
    gpio1: GPIO1,
    gpio2: GPIO2,
    gpio4: GPIO4,
}

/// 信号名で束ねたカートリッジ用ピン。
pub struct CartPins {
    pub d0: P14,
    pub d1: P15,
    pub d2: P40,
    pub d3: P41,
    pub d4: P17,
    pub d5: P16,
    pub d6: P22,
    pub d7: P23,
    pub a0: P19,
    pub a1: P18,
    pub a2: P38,
    pub a3: P39,
    pub a4: P24,
    pub a5: P25,
    pub a6: P0,
    pub a7: P1,
    pub a8: P20,
    pub a9: P21,
    pub a10: P2,
    pub a11: P3,
    pub a12: P4,
    pub a13: P5,
    pub a14: P6,
    pub a15: P8,
    pub n_rd: P33,
    pub n_wr: P34,
    pub n_cs: P35,
    pub n_reset: P37,
}

const DATA_SHIFT: u32 = 18;
const DATA_MASK: u32 = 0xFF << DATA_SHIFT;

/// A0-A9 → GPIO1 のビット位置
const A0_A9_BITS: [u32; 10] = [16, 17, 28, 29, 12, 13, 3, 2, 26, 27];
/// A10-A13 → GPIO4 のビット位置
const A10_A13_BITS: [u32; 4] = [4, 5, 6, 8];
/// A14, A15 → GPIO2 のビット位置
const A14_A15_BITS: [u32; 2] = [10, 16];

const GPIO1_ADDR_MASK: u32 = scatter(0xFFFF, 0, &A0_A9_BITS);
const GPIO4_ADDR_MASK: u32 = scatter(0xFFFF, 10, &A10_A13_BITS);
const GPIO2_ADDR_MASK: u32 = scatter(0xFFFF, 14, &A14_A15_BITS);

const N_RD: u32 = 1 << 7; // GPIO4
const N_WR: u32 = 1 << 29; // GPIO2
const N_CS: u32 = 1 << 28; // GPIO2
const N_RESET: u32 = 1 << 19; // GPIO2

/// /RD または /WR をアサートしてからデータが確定するまでの待ち (≈300ns @600MHz)。
/// ROM のアクセスタイム (150ns 前後) ぎりぎりには詰めていない: 245 を往復する伝搬遅延と
/// MBC のデコード遅延の分を見込んだ余裕値。実機で詰めるなら負荷 % を見ながら縮める。
const ACCESS_DELAY: u32 = 180;

/// /RESET の L 保持時間と解除後の安定待ち (1ms @600MHz)。
const RESET_DELAY: u32 = 600_000;

/// `addr` のビット `first..` を、`gpio_bits` で指定した GPIO ビット位置へ散らす。
const fn scatter(addr: u16, first: u32, gpio_bits: &[u32]) -> u32 {
    let mut out = 0;
    let mut i = 0;
    while i < gpio_bits.len() {
        if addr & (1 << (first + i as u32)) != 0 {
            out |= 1 << gpio_bits[i];
        }
        i += 1;
    }
    out
}

/// `mask` 内のビットを `bits` の値にする。DR の read-modify-write を避け、
/// 同じポートの他のピン (ディスプレイ DC、ボタン走査線) を割り込みと競合させない。
macro_rules! put_bits {
    ($gpio:expr, $mask:expr, $bits:expr) => {{
        ral::write_reg!(ral::gpio, $gpio, DR_CLEAR, $mask & !$bits);
        ral::write_reg!(ral::gpio, $gpio, DR_SET, $bits);
    }};
}

impl GpioCart {
    /// ピンを GPIO に設定し、カートリッジにリセットパルスを与える。
    ///
    /// `pins.d7` (p23) は SAI1 の MCLK にも割り当てられているため、
    /// オーディオ初期化 **後** に呼んで GPIO へ切り替え直すこと。
    pub fn new(
        gpio1: &mut Port<1>,
        gpio2: &mut Port<2>,
        gpio4: &mut Port<4>,
        pins: CartPins,
    ) -> Self {
        // Safety: 以後このドライバは DR_SET/DR_CLEAR/GDIR/PSR の担当ビットしか触らない。
        let cart = unsafe {
            Self {
                gpio1: GPIO1::instance(),
                gpio2: GPIO2::instance(),
                gpio4: GPIO4::instance(),
            }
        };

        // 出力へ切り替えた瞬間に制御線が L (アサート) で出ないよう、先に H を書いておく。
        // /RESET だけはここで L にしてリセットを開始する。
        ral::write_reg!(ral::gpio, cart.gpio4, DR_SET, N_RD);
        ral::write_reg!(ral::gpio, cart.gpio2, DR_SET, N_WR | N_CS);
        ral::write_reg!(ral::gpio, cart.gpio2, DR_CLEAR, N_RESET);

        // output()/input() は IOMUXC を GPIO モードに設定する。戻り値のハンドルは
        // 保持しない (以後はビットマスクでまとめて操作する)。
        let CartPins {
            d0, d1, d2, d3, d4, d5, d6, d7,
            a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15,
            n_rd, n_wr, n_cs, n_reset,
        } = pins;
        gpio1.input(d0);
        gpio1.input(d1);
        gpio1.input(d2);
        gpio1.input(d3);
        gpio1.input(d4);
        gpio1.input(d5);
        gpio1.input(d6);
        gpio1.input(d7);
        gpio1.output(a0);
        gpio1.output(a1);
        gpio1.output(a2);
        gpio1.output(a3);
        gpio1.output(a4);
        gpio1.output(a5);
        gpio1.output(a6);
        gpio1.output(a7);
        gpio1.output(a8);
        gpio1.output(a9);
        gpio4.output(a10);
        gpio4.output(a11);
        gpio4.output(a12);
        gpio4.output(a13);
        gpio2.output(a14);
        gpio2.output(a15);
        gpio4.output(n_rd);
        gpio2.output(n_wr);
        gpio2.output(n_cs);
        gpio2.output(n_reset);

        asm::delay(RESET_DELAY);
        ral::write_reg!(ral::gpio, cart.gpio2, DR_SET, N_RESET);
        asm::delay(RESET_DELAY);

        cart
    }

    #[inline(always)]
    fn set_address(&self, addr: u16) {
        put_bits!(self.gpio1, GPIO1_ADDR_MASK, scatter(addr, 0, &A0_A9_BITS));
        put_bits!(self.gpio4, GPIO4_ADDR_MASK, scatter(addr, 10, &A10_A13_BITS));
        put_bits!(self.gpio2, GPIO2_ADDR_MASK, scatter(addr, 14, &A14_A15_BITS));
    }

    /// 外部 RAM 域 (0xA000-0xBFFF) のアクセス中だけ /CS をアサートする (実機 GB と同じ)。
    #[inline(always)]
    fn select_ram(&self, addr: u16, assert: bool) {
        if addr >= 0xA000 {
            if assert {
                ral::write_reg!(ral::gpio, self.gpio2, DR_CLEAR, N_CS);
            } else {
                ral::write_reg!(ral::gpio, self.gpio2, DR_SET, N_CS);
            }
        }
    }
}

impl CartridgeBus for GpioCart {
    fn read(&self, addr: u16) -> u8 {
        self.set_address(addr);
        self.select_ram(addr, true);

        // /RD=L で 245 の DIR が Cart→Teensy に切り替わる。データピンは常時入力なので衝突しない。
        ral::write_reg!(ral::gpio, self.gpio4, DR_CLEAR, N_RD);
        asm::delay(ACCESS_DELAY);
        let val = (ral::read_reg!(ral::gpio, self.gpio1, PSR) >> DATA_SHIFT) as u8;
        ral::write_reg!(ral::gpio, self.gpio4, DR_SET, N_RD);

        self.select_ram(addr, false);
        val
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.set_address(addr);

        // /RD=H (DIR=Teensy→Cart) の間だけデータピンを出力にする。
        put_bits!(self.gpio1, DATA_MASK, (val as u32) << DATA_SHIFT);
        ral::modify_reg!(ral::gpio, self.gpio1, GDIR, |d| d | DATA_MASK);
        self.select_ram(addr, true);

        ral::write_reg!(ral::gpio, self.gpio2, DR_CLEAR, N_WR);
        asm::delay(ACCESS_DELAY);
        ral::write_reg!(ral::gpio, self.gpio2, DR_SET, N_WR);

        self.select_ram(addr, false);
        ral::modify_reg!(ral::gpio, self.gpio1, GDIR, |d| d & !DATA_MASK);
    }
}
