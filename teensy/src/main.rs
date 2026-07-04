#![no_std]
#![no_main]

mod audio;
mod cartridge;
mod display;
mod input;
mod sdcard;

use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::board;
#[allow(unused_imports)]
use bsp::interrupt;

use gb_core::{bootrom::Bootrom, gameboy::GameBoy, mmu::Mmu, platform::NullAudio};

// --- USB シリアルログ ---
struct UsbPollerCell(core::cell::UnsafeCell<Option<imxrt_log::Poller>>);
unsafe impl Sync for UsbPollerCell {}
static USB_POLLER: UsbPollerCell = UsbPollerCell(core::cell::UnsafeCell::new(None));

#[bsp::rt::interrupt]
fn USB_OTG1() {
    unsafe {
        if let Some(poller) = (*USB_POLLER.0.get()).as_mut() {
            poller.poll();
        }
    }
}

#[bsp::rt::interrupt]
fn SAI1() {
    audio::on_sai1_interrupt();
}

use display::panel::St7789;
use display::DmaDisplay;
use input::GpioInput;
use sdcard::FlashCart;

/// Teensy 4.1 全ピン割り当て (確定):
///
/// ┌ Display (LPSPI4) ──────────────────────────────────────────────┐
/// │ MOSI=11  MISO=12  SCK=13  CS=10(PCS0)  DC=9  RST=8  BL=3.3V直結  │
/// ├ Cartridge (GpioCart) ──────────────────────────────────────────┤
/// │ D0-D7 = 14,15,40,41,17,16,22,23     (GPIO1[18-25] 連続・高速読出) │
/// │ A0-A9 = 19,18,38,39,24,25,0,1,20,21 (全て GPIO1。bit は非連続)    │
/// │ A10-A14 = 2,3,4,5,6                  (GPIO4/GPIO2)               │
/// │ /RD=33  /WR=34  /CS=35   /RESET=37 (GPIO2[19], MBC リセット用)  │
/// │   ※ A15 は不使用 (ROM 域は常に 0、外部RAM は /CS で選択)         │
/// │   ※ /RESET は電源投入後に L→H パルスで MBC バンクレジスタを初期化 │
/// ├ Audio (SAI1 TX / PCM5102) ─────────────────────────────────────┤
/// │ TX_DATA=7   TX_BCLK=26   TX_SYNC=27                             │
/// ├ Buttons (2x4 マトリクス, GB 準拠) ──────────────────────────────┤
/// │ SEL_DIR=28  SEL_ACT=29   IN0-IN3 = 30,31,32,36                  │
/// │   SEL_DIR=LOW → 右/左/上/下,  SEL_ACT=LOW → A/B/Select/Start     │
/// └────────────────────────────────────────────────────────────────┘
///
/// 実機検証で判明した配線の重要事項 (詳細は docs/teensy_setup_guide.md):
///   - 単一 SPI デバイスなら CS→GND, RESET→3.3V 固定が最も確実 (その場合 p10/p8 は未使用)。
///   - GB カートリッジは 5V 系 → D/A/制御線は 74AHCT245 等でレベル変換が必要。
///
/// ROM は Flash に埋め込み (include_bytes!)。
/// SDカード対応は docs/teensy_setup_guide.md を参照。

// ROM は Flash に埋め込む。
// デフォルトは roms/game.gb。GB_ROM 環境変数または Makefile の ROM 変数で上書き可能:
//   make ROM=/path/to/game.gbc build
static ROM: &[u8] = include_bytes!(env!("GB_ROM_PATH"));

#[bsp::rt::entry]
fn main() -> ! {
    let board::Resources {
        usb,
        lpspi4,
        sai1,
        mut gpio2,
        mut gpio3,
        mut gpio4,
        mut dma,
        pins,
        ..
    } = board::t41(board::instances());

    let mut cp = cortex_m::Peripherals::take().unwrap();

    // ------- L1 キャッシュ有効化 -------
    // Cortex-M7 はリセット時キャッシュ無効。ROM は Flash の XIP 配置 (build.rs) のため、
    // D-cache が無いと GB の命令フェッチごとに低速な FlexSPI アクセスが発生し激遅になる。
    // DMA 対象のフレームバッファ FB は DTCM (非キャッシュの TCM) にあるため、
    // D-cache を有効化しても DMA とのコヒーレンシ問題は生じない。
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    // ------- DWT サイクルカウンタ有効化 (フレームペーシングのタイマー) -------
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    // ------- USB シリアルログ (imxrt-log) -------
    let poller = imxrt_log::log::usbd(usb, imxrt_log::Interrupts::Enabled).unwrap();
    unsafe {
        *USB_POLLER.0.get() = Some(poller);
        cortex_m::peripheral::NVIC::unmask(bsp::interrupt::USB_OTG1);
    }

    // ------- ROM (Flash 埋め込み) -------
    let cart = FlashCart::new(ROM);

    // ------- ILI9341 ディスプレイ (LPSPI4) -------
    let spi: board::Lpspi4 = board::lpspi(
        lpspi4,
        board::LpspiPins {
            sdo: pins.p11,
            sdi: pins.p12,
            sck: pins.p13,
            pcs0: pins.p10,
        },
        // BSP の set_spi_clock は分周 half_div を下限3でクランプ → SCKDIV=4 固定。
        // SPI = 132MHz/(4+2) = 約22MHz が実効上限で、ここに何を渡しても 22MHz になる。
        // 実クロックは直後の CCR 直書きで設定するため、この値は形式的なもの。
        24_000_000,
    );

    // BSP のクランプを外し、CCR を直接書き換えて SPI を高速化する。
    // SCKDIV=2 → 132MHz/(2+2) = 33MHz。これで全画面 DMA 転送が約11msに収まり、
    // フレーム予算(16.7ms)の裏に完全に隠れる (実機計測で wait=0 を確認)。
    // CCR は LPSPI 無効時のみ書けるため MEN をトグルする。DBT/PCSSCK/SCKPCS は
    // half_div=2 相当の 1 (クランプが無ければ hal が算出したはずの値と同一)。
    unsafe {
        const LPSPI4_BASE: u32 = 0x403A_0000;
        let cr = (LPSPI4_BASE + 0x10) as *mut u32; // 制御レジスタ
        let ccr = (LPSPI4_BASE + 0x40) as *mut u32; // クロック構成レジスタ
        const SCKDIV: u32 = 2; // 132/(2+2)=33MHz。22MHz に戻すなら 4
        const DLY: u32 = 1; // DBT/PCSSCK/SCKPCS (= half_div-1)

        let men = core::ptr::read_volatile(cr) & 1;
        core::ptr::write_volatile(cr, core::ptr::read_volatile(cr) & !1); // MEN=0
        core::ptr::write_volatile(
            ccr,
            (DLY << 24) | (DLY << 16) | (DLY << 8) | SCKDIV, // SCKPCS|PCSSCK|DBT|SCKDIV
        );
        core::ptr::write_volatile(cr, core::ptr::read_volatile(cr) | men); // MEN 復帰
    }

    let dc = gpio2.output(pins.p9);
    let rst = gpio2.output(pins.p8);
    let dma_channel = dma[0].take().unwrap();

    let display = DmaDisplay::<St7789, _, _, _>::new(spi, dc, rst, dma_channel);

    // ------- SAI1 オーディオ (MAX98357A, I2S) -------
    // 既知の問題により無効化中: SAI1 を有効化するとランダムなタイミングで
    // 画面が真っ黒になる (アンプ・カートリッジ回路の有無、初期化順序とは無関係)。
    // 詳細・調査経緯は docs/teensy_setup_guide.md の「既知の問題」節を参照。
    let _ = sai1;
    let audio = NullAudio;

    // ------- GB コア -------

    let bootrom = Bootrom::disabled();

    // ボタン入力 (2x4 マトリクス)。ピンの GPIO ポートは型で固定されている。
    let input = GpioInput::new(
        &mut gpio2, &mut gpio3, &mut gpio4, pins.p28, pins.p29, pins.p30, pins.p31, pins.p32,
        pins.p36,
    );
    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, display, audio, input);

    // ------- メインループ (フレームペーシング) -------
    // GB 1 フレーム = 70224 T-cycle。ARM クロック換算のフレーム周期 (約16.742ms) ごとに
    // ループを同期させ、emu/描画がどれだけ速くても realtime (59.7fps) に固定する。
    use cortex_m::peripheral::DWT;
    const FRAME_CYCLES: u32 = (board::ARM_FREQUENCY as u64 * 70224 / 4_194_304) as u32;
    let mut next_deadline = DWT::cycle_count().wrapping_add(FRAME_CYCLES);
    // フレーム処理開始時刻 (ビジーウェイト解除直後)。実処理サイクル計測の基準。
    let mut frame_start = DWT::cycle_count();

    loop {
        let r = gb.step();

        if r.frame_ready {
            let now = DWT::cycle_count();
            // このフレームの実処理サイクル (step 群 + draw、待機を含まない) を記録。
            // オーバーレイの2行目に負荷% とコマ落ち回数として表示される。
            let work = now.wrapping_sub(frame_start);
            gb.display_mut().record_work(work, FRAME_CYCLES);

            if (now.wrapping_sub(next_deadline) as i32) >= 0 {
                // 締切超過 (処理が予算を上回った) → 同期しなおしてバースト追い上げを防ぐ
                next_deadline = now.wrapping_add(FRAME_CYCLES);
            } else {
                // 締切まで待機して realtime に同期
                while (DWT::cycle_count().wrapping_sub(next_deadline) as i32) < 0 {}
                next_deadline = next_deadline.wrapping_add(FRAME_CYCLES);
            }
            // 待機を終えた地点を次フレームの処理開始基準にする。
            frame_start = DWT::cycle_count();
        }
    }
}
