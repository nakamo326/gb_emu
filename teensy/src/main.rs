#![no_std]
#![no_main]

mod audio;
#[cfg(feature = "real-cart")]
mod cartridge;
mod display;
mod input;
#[cfg(not(feature = "real-cart"))]
mod sdcard;

use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::board;
#[allow(unused_imports)]
use bsp::interrupt;

use gb_core::{bootrom::Bootrom, gameboy::GameBoy, mmu::Mmu, platform::CartridgeBus};

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

/// Teensy 4.1 全ピン割り当て (確定):
///
/// ┌ Display (LPSPI4) ──────────────────────────────────────────────┐
/// │ MOSI=11  MISO=12  SCK=13  CS=10(PCS0)  DC=9  RST/BL=3.3V直結     │
/// ├ Cartridge (GpioCart) ──────────────────────────────────────────┤
/// │ D0-D7 = 14,15,40,41,17,16,22,23     (GPIO1[18-25] 連続・高速読出) │
/// │ A0-A9 = 19,18,38,39,24,25,0,1,20,21 (全て GPIO1。bit は非連続)    │
/// │ A10-A14 = 2,3,4,5,6                  (GPIO4/GPIO2)               │
/// │ A15 = 8 (GPIO2[16]。ディスプレイ RST から転用)                  │
/// │ /RD=33  /WR=34  /CS=35   /RESET=37 (GPIO2[19], MBC リセット用)  │
/// │   ※ D7(p23) は SAI1_MCLK と共用。オーディオ初期化後に GPIO へ戻す │
/// │   ※ /RESET は電源投入後に L→H パルスで MBC バンクレジスタを初期化 │
/// ├ Audio (SAI1 TX / PCM5102) ─────────────────────────────────────┤
/// │ TX_DATA=7   TX_BCLK=26   TX_SYNC=27                             │
/// ├ Buttons (2x4 マトリクス, GB 準拠) ──────────────────────────────┤
/// │ SEL_DIR=28  SEL_ACT=29   IN0-IN3 = 30,31,32,36                  │
/// │   SEL_DIR=LOW → 右/左/上/下,  SEL_ACT=LOW → A/B/Select/Start     │
/// └────────────────────────────────────────────────────────────────┘
///
/// 実機検証で判明した配線の重要事項 (詳細は docs/teensy_setup_guide.md):
///   - 単一 SPI デバイスなら CS→GND, RESET→3.3V 固定が最も確実 (p8 は A15 に転用済み)。
///   - GB カートリッジは 5V 系 → SN74LVC16T245 ×2 でレベル変換する。
///
/// ROM の供給元は feature で切り替える:
///   - デフォルト: Flash に埋め込んだ ROM (include_bytes!)。
///   - `real-cart`: GPIO バス経由で実カートリッジを読む (`make FEATURES=real-cart build`)。
/// SDカード対応は docs/teensy_setup_guide.md を参照。

// デフォルトは roms/game.gb。GB_ROM 環境変数または Makefile の ROM 変数で上書き可能:
//   make ROM=/path/to/game.gbc build
#[cfg(not(feature = "real-cart"))]
static ROM: &[u8] = include_bytes!(env!("GB_ROM_PATH"));

/// ヘッダチェックサム (0x14D) を検証する。実カートでは未挿入・配線不良・タイミング不足の検出に使う。
fn header_checksum_ok(cart: &impl CartridgeBus) -> bool {
    let sum = (0x134..=0x14C).fold(0u8, |x, a| x.wrapping_sub(cart.read(a)).wrapping_sub(1));
    sum == cart.read(0x14D)
}

#[bsp::rt::entry]
fn main() -> ! {
    let board::Resources {
        usb,
        lpspi4,
        sai1,
        #[cfg(feature = "real-cart")]
        mut gpio1,
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
    let dma_channel = dma[0].take().unwrap();

    let display = DmaDisplay::<St7789, _, _>::new(spi, dc, dma_channel);

    // ------- SAI1 オーディオ (MAX98357A/PCM5102A, I2S) -------
    // 既知の問題: 有効化するとランダムなタイミングで画面が真っ黒になる未解決バグがある。
    // 詳細・調査経緯は docs/teensy_setup_guide.md の「既知の問題」節を参照。
    let audio = audio::SaiAudio::new(sai1, pins.p7, pins.p23, pins.p26, pins.p27);
    unsafe {
        cortex_m::peripheral::NVIC::unmask(bsp::interrupt::SAI1);
    }

    // ------- GB コア -------

    let bootrom = Bootrom::disabled();

    // ボタン入力 (2x4 マトリクス)。ピンの GPIO ポートは型で固定されている。
    let input = GpioInput::new(
        &mut gpio2, &mut gpio3, &mut gpio4, pins.p28, pins.p29, pins.p30, pins.p31, pins.p32,
        pins.p36,
    );

    // ------- カートリッジ -------
    #[cfg(not(feature = "real-cart"))]
    let cart = sdcard::FlashCart::new(ROM);

    // p23 (D7) は SaiAudio::new() が SAI1_MCLK として IOMUXC を設定済み。MCLK は未配線のため、
    // ここで GPIO に切り替え直してデータバスとして使う (オーディオより後に初期化する理由)。
    #[cfg(feature = "real-cart")]
    let cart = cartridge::GpioCart::new(
        &mut gpio1,
        &mut gpio2,
        &mut gpio4,
        cartridge::CartPins {
            d0: pins.p14,
            d1: pins.p15,
            d2: pins.p40,
            d3: pins.p41,
            d4: pins.p17,
            d5: pins.p16,
            d6: pins.p22,
            // Safety: SAI ドライバは MCLK ピンを保持するだけで以後触らない。
            d7: unsafe { bsp::pins::t41::P23::new() },
            a0: pins.p19,
            a1: pins.p18,
            a2: pins.p38,
            a3: pins.p39,
            a4: pins.p24,
            a5: pins.p25,
            a6: pins.p0,
            a7: pins.p1,
            a8: pins.p20,
            a9: pins.p21,
            a10: pins.p2,
            a11: pins.p3,
            a12: pins.p4,
            a13: pins.p5,
            a14: pins.p6,
            a15: pins.p8,
            n_rd: pins.p33,
            n_wr: pins.p34,
            n_cs: pins.p35,
            n_reset: pins.p37,
        },
    );

    let checksum_ok = header_checksum_ok(&cart);
    log::info!(
        "cart: cgb_flag=0x{:02X} cart_type=0x{:02X} rom_size=0x{:02X} header_checksum={}",
        cart.read(0x143),
        cart.read(0x147),
        cart.read(0x148),
        if checksum_ok { "ok" } else { "NG" }
    );
    #[cfg(feature = "real-cart")]
    if !checksum_ok {
        panic!("cartridge header checksum mismatch (not inserted, wiring, or bus timing)");
    }

    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, display, audio, input);

    // ------- メインループ (フレームペーシング) -------
    // 一次ペーシングはオーディオが担う: `SaiAudio::push()` がリングバッファ満杯時に
    // ブロックするため、エミュレーションは SAI の実消費レート (≈44117 Hz、フレーム換算
    // ≈59.75fps) に正確にロックされる。LCD オフ期間中 (frame_ready が来ない間) も効く。
    // DWT の締切は名目フレーム周期より 5% 速いセーフティキャップに留め、通常は発動しない。
    // 役割は (1) オーディオ停止時 (push がドロップ動作へ退避した場合) の暴走防止、
    // (2) アンダーラン後にバッファを再充填する際の追い上げ速度の上限 (+5%)。
    use cortex_m::peripheral::DWT;
    const FRAME_CYCLES_NOMINAL: u32 = (board::ARM_FREQUENCY as u64 * 70224 / 4_194_304) as u32;
    const FRAME_CYCLES_CAP: u32 = (FRAME_CYCLES_NOMINAL as u64 * 100 / 105) as u32;
    let mut next_deadline = DWT::cycle_count().wrapping_add(FRAME_CYCLES_CAP);
    // フレーム処理開始時刻 (ビジーウェイト解除直後)。実処理サイクル計測の基準。
    let mut frame_start = DWT::cycle_count();

    loop {
        let r = gb.step();

        if r.frame_ready {
            let now = DWT::cycle_count();
            // このフレームの実処理サイクル (step 群 + draw) を記録。オーディオペーシングの
            // 待機時間は除外する。オーバーレイの2行目に負荷% とコマ落ち回数として表示される。
            let work = now
                .wrapping_sub(frame_start)
                .saturating_sub(audio::SaiAudio::take_blocked_cycles());
            gb.display_mut().record_work(work, FRAME_CYCLES_NOMINAL);

            if (now.wrapping_sub(next_deadline) as i32) >= 0 {
                // 締切超過。オーディオペーシングが効いている通常運転では毎フレーム
                // ここに入る (キャップは名目より速いため)。同期しなおすだけで良い。
                next_deadline = now.wrapping_add(FRAME_CYCLES_CAP);
            } else {
                // オーディオが仕事をしていない (停止 or バッファ再充填中) 場合のみ
                // ここに来る。キャップ (+5%) まで待機して暴走を防ぐ。
                while (DWT::cycle_count().wrapping_sub(next_deadline) as i32) < 0 {}
                next_deadline = next_deadline.wrapping_add(FRAME_CYCLES_CAP);
            }
            // 待機を終えた地点を次フレームの処理開始基準にする。
            frame_start = DWT::cycle_count();
        }
    }
}
