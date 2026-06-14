use imxrt_rt::{Family, FlexRamBanks, Memory, RuntimeBuilder};

fn main() {
    // i.MX RT1062 (Teensy 4.1) 向けランタイムを生成する。
    // FCB / IVT / boot data の配置・FlexRAM バンク設定・リンカスクリプト (t4link.x) を
    // imxrt-rt が自動生成するため、手書きの memory.x は不要。
    //
    // Teensy 4.1 の QSPI Flash は 8 MB。Arduino が報告する実用プログラム領域に合わせて
    // 7936 KiB (= 8,126,464 byte) を確保する (末尾は EEPROM エミュレーション等に使われる)。
    RuntimeBuilder::from_flexspi(Family::Imxrt1060, 7936 * 1024)
        // FlexRAM 512 KB = 16 バンク(各32KB)を ITCM/DTCM に配分 (OCRAM は専用 OCRAM2 を使用)
        .flexram_banks(FlexRamBanks {
            ocram: 0,
            itcm: 6,  // 192 KB: コード(.text)
            dtcm: 10, // 320 KB: ベクタ・スタック・静的変数・フレームバッファ
        })
        .stack(Memory::Dtcm)
        .stack_size(16 * 1024)
        .stack_size_env_override("TEENSY4_STACK_SIZE")
        .vectors(Memory::Dtcm)
        .text(Memory::Itcm)
        // ROM は include_bytes! で .rodata に埋め込まれ最大数 MB になり得るため、
        // RAM にコピーせず Flash 上 (XIP) に置く。
        //
        // 【将来】ROM を SDカードや実カートリッジから読むようになり include_bytes! を
        // やめたら、.rodata は小さな定数のみになる。その場合はこの行を削除して
        // imxrt-rt のデフォルト (OCRAM) に戻すと、オンチップ RAM 読み出しで高速化できる。
        // 詳細: docs/teensy_setup_guide.md「rodata のメモリ配置」
        .rodata(Memory::Flash)
        .data(Memory::Dtcm)
        .bss(Memory::Dtcm)
        .uninit(Memory::Ocram)
        // teensy4-bsp の rt フィーチャも "t4link.x" を生成するため、別名にして
        // リンク検索順に依存せず確実に本スクリプトが選ばれるようにする。
        .linker_script_name("gb-teensy-link.x")
        .build()
        .unwrap();
}
