# Teensy 4.1 移植 — 現状と残タスク

作成日: 2026-06-10  
最終更新: 2026-06-24

---

## 概要

Game Boy エミュレータを **Teensy 4.1**（i.MX RT1062, Cortex-M7, 600 MHz）でも動作させるため、
単一コードベースを以下の 3 クレートに分割した。

```
gb_emu/
├── Cargo.toml          # ワークスペース (members: core, host; teensy は exclude)
├── core/               # gb-core : #![no_std], ヒープ不使用
├── host/               # gb-host : std + SDL2 (Linux/macOS デスクトップ向け)
└── teensy/             # gb-teensy : #![no_std] #![no_main], thumbv7em-none-eabihf 専用
```

---

## 完了済み作業

### 1. Cargo ワークスペース化

- ルート `Cargo.toml` を `[workspace]` に変換し `core/`・`host/` を members に登録
- 旧 `src/` の全ファイルを `core/src/` へ移設

### 2. `gb-core` の no_std 化

**`core/src/lib.rs`**
```rust
#![cfg_attr(not(test), no_std)]
```
テスト時のみ std を使用し、本体ビルドは完全 no_std。

**プラットフォーム抽象 (`core/src/platform.rs`)**

| トレイト | 役割 |
|---|---|
| `Display` | `draw(&[u8])` — 160×144 パレットインデックスバッファを表示 |
| `AudioSink` | `push(l: f32, r: f32)` — ステレオサンプルを出力先へ |
| `CartridgeBus` | `read/write` — ROM/MBC/外部RAM バスアクセス |
| `InputSource` | `poll() -> ButtonState` — ボタン状態取得 (`core/src/input.rs`) |

no-op 実装（`NullDisplay`, `NullAudio`, `NullCartridge`, `NullInput`）を同梱。

**`MemoryBus` トレイト (`core/src/mmu.rs`)**

CPU・命令テーブル (`ExecFn`) を非ジェネリックに保つため `Mmu<C>` を型消去する
`dyn MemoryBus` 抽象を導入。`ExecFn = fn(&mut Cpu, &mut dyn MemoryBus) -> bool` の形。

### 3. ヒープ除去

| ファイル | 変更 |
|---|---|
| `ppu.rs` | `vram/oam: Box<[u8;N]>` → 固定配列; `sprite_buffer: Vec` → `heapless::Vec<SpriteData, 10>` |
| `apu.rs` | `emulate_cycle() -> Vec<f32>` → `-> Option<(f32, f32)>` |
| `bootrom.rs` | `File`/`io` 依存を除去。`from_bytes([u8;0x100])` と `disabled()` のみ |
| `mmu.rs` | `test-harness` フィーチャーで `TestHarness`（blargg 検知）を条件コンパイル。内部バッファは `heapless::Vec<u8, 8192>` |
| `cartridge.rs` | MBC エミュ（`Vec<u8>` ROM + `Box<dyn MemoryBankController>`）を `host/` へ移設 |

`SpriteData` に `order: u8` フィールドを追加し、`sort_unstable_by` で安定ソートを再現
（`heapless::Vec` は `sort_by_key` 非対応のため）。

### 4. `GameBoy` のリファクタ (`core/src/gameboy.rs`)

```rust
pub struct GameBoy<C: CartridgeBus, D: Display, A: AudioSink, I: InputSource> {
    cpu: Cpu, mmu: Mmu<C>, display: D, audio: A, input: I,
}

pub fn step(&mut self) -> StepResult  // 1 M-cycle 進める
```

- タイミング制御（スリープ等）をコアから除去し `step()` を公開
- `StepResult { frame_ready: bool, quit: bool }` でフレーム完成と終了要求を通知
- 戻り値で `display.draw()` → `input.poll()` → `audio.push()` をフレームタイミングで呼ぶ

### 5. `gb-host` の実装

**`host/src/cartridge.rs`**  
既存 MBC エミュレーション（`RomOnly`, `Mbc1`, `Mbc3`, `Mbc5`）に対し
`impl CartridgeBus for Cartridge` を追加。

**`host/src/lcd.rs`**  
SDL2 状態を 3 ストラクトに分割し、各トレイトを個別実装:

| 型 | 実装トレイト | 保持するもの |
|---|---|---|
| `SdlDisplay` | `Display` | `Canvas<Window>`, `Sdl` コンテキスト |
| `SdlAudio` | `AudioSink` | `Option<AudioQueue<f32>>` |
| `SdlInput` | `InputSource` | `EventPump` |

`create_sdl_backends() -> (SdlDisplay, SdlAudio, SdlInput)` で 3 つをまとめて初期化。

**`host/src/main.rs`**  
- wall-clock catch-up 方式のメインループ（`M_CYCLE_NS = 4×10⁹/4_194_304 ≈ 954 ns`）
- `run_headless()` でタイミングなし全力実行 + test-harness シリアル出力を表示

### 6. 検証

| チェック | 結果 |
|---|---|
| `cargo test -p gb-core -- --test-threads=1` | **21 テスト全通過** |
| `cargo build -p gb-core --target thumbv7em-none-eabihf` | **ビルド成功** |
| `cargo build`（ワークスペース全体） | 警告のみ・エラーなし |

### 7. Teensy 実機での性能最適化（2026-06-24）

実機でカービィのデモが明らかに遅かった（約 20fps）問題を、DWT サイクルカウンタ
＋ USB シリアルログによる計測で切り分け、以下を実施した。計測の結果、当初の想定と
異なり **描画(SPI/DMA)ではなく CPU エミュレーションが律速**であることが判明した。

| 施策 | 効果 | 実装 |
|---|---|---|
| **L1 I/D キャッシュ有効化** | emu **48ms→5ms**（約10倍）、fps 20→59 | `main.rs` 起動時に `enable_icache`/`enable_dcache` |
| **SPI クロック 33MHz 化** | 全画面 DMA 転送 16.7ms→約11ms、`wait=0` | `main.rs` で CCR を直書きし BSP のクランプ回避 |
| **フレームペーシング** | **59.7fps に固定**＋idle 約11.5ms の余裕 | `main.rs` メインループを DWT で 70224 T-cycle 周期に同期 |

**最大の原因はキャッシュ無効**だった。ROM は Flash の XIP 配置（`build.rs` の
`.rodata(Memory::Flash)`）のため、D-cache 無効だと GB の命令フェッチごとに低速な
FlexSPI アクセスが発生していた。Cortex-M7 はリセット時キャッシュ無効で、bare-metal
（imxrt-rt + bsp）構成では誰も有効化していなかった。DMA 対象のフレームバッファ FB は
非キャッシュの DTCM (TCM) にあるため、D-cache 有効化でも DMA コヒーレンシ問題は生じない。

**SPI クロックの制約**: BSP の `set_spi_clock` は分周 `half_div` を下限3でクランプ
するため SPI = `132MHz/(4+2) ≈ 22MHz` が実効上限で、`board::lpspi(...)` に何 Hz を
渡しても 22MHz になる。これはハードや配線の限界ではなくライブラリのソフト制限のため、
LPSPI4 の CCR レジスタ（オフセット 0x40）を直接書き換えて `SCKDIV=2`（33MHz）に設定。
実機で 33MHz でも表示は乱れないことを確認済み（continuous mode で乱れる場合は 22MHz に戻す）。

**ペーシングの必要性**: teensy ループは従来ペース調整が無く、SPI 転送時間が偶然
フレーム予算に近かったため概ね正しい速度だった。33MHz 化で転送が予算より大幅に短く
なり realtime より速く動いてしまうため、DWT で 1 フレーム周期に同期させて固定した。

計測足場（USB シリアルログ・DWT 集計）は切り分け後に撤去済み。詳細なコミット:
`cache有効化` → `性能計測の追加/撤去` → `SPIクロック33MHz` → `フレームペーシング`。

---

## 残タスク

### A. `cargo run -p gb-host` で実機 PC 動作確認（優先度: 高）

`dmg_bootrom.bin` と ROM を用意して実際に画面が出るか確認する。
blargg テスト（`cpu_instrs.gb --headless`）で出力が従来通りかも確認。

```bash
# dmg_bootrom.bin を作業ディレクトリに置いてから
cargo run -p gb-host -- cpu_instrs.gb --headless
cargo run -p gb-host -- test_rom.gb
```

---

### B. `teensy/` クレートの骨組み ✅ 完了

**実装済みファイル構成:**

```
teensy/
├── Cargo.toml
├── Makefile                 # build / hex / flash (WSL→Windows teensy_loader 対応)
├── .cargo/
│   └── config.toml          # target = thumbv7em-none-eabihf, runner
├── build.rs                 # imxrt-rt RuntimeBuilder でリンカスクリプト等を生成
└── src/
    ├── main.rs              # #![no_std] #![no_main] + エントリポイント
    ├── display/
    │   ├── mod.rs           # DMA 描画ドライバ (DmaDisplay)
    │   └── panel.rs         # パネル抽象 (PanelController / Ili9341)
    ├── sdcard.rs            # FlashCart (Flash 埋め込み ROM, RomOnly/MBC1)
    ├── cartridge.rs         # GpioCart (実 GB カートリッジ GPIO バス)
    ├── audio.rs             # I2S オーディオ (スタブ)
    └── input.rs             # GPIO ボタン入力 (スタブ)
```

**ビルドシステム**: 手書きの `memory.x` は廃止。`build.rs` が `imxrt-rt` の
`RuntimeBuilder` で FCB/IVT/FlexRAM 設定とリンカスクリプト（`gb-teensy-link.x`）を
自動生成する。FlexRAM 512KB は ITCM 192KB（`.text`）/ DTCM 320KB（スタック・静的変数・
フレームバッファ）に配分。ROM は `.rodata(Memory::Flash)` で Flash 上に XIP 配置。

ビルド・書き込みは `Makefile` 経由（`make build` / `make hex` / `make flash`）。
WSL からは `make flash TEENSY_CLI=.../teensy_loader_cli.exe` で Windows 側に書き込む。

---

### C. ILI9341 Display 実装 ✅ 完了（DMA 描画 + パネル抽象）

`teensy/src/display/mod.rs` に `DmaDisplay<P, SPI, DC, RST>`、`display/panel.rs` に
パネル抽象 `PanelController`（実装 `Ili9341`）として実装済み。

**実装内容:**
- 描画は **LPSPI4 + eDMA** によるバックグラウンド転送（CPU を専有しない）
- **ダブルバッファ**（FB×2）。`draw()` は前回 DMA 完了待ち → 変換 → 次 DMA 起動
- continuous mode で 2px を 1 u32 にパックし 11520 ワードを転送（CCR/TCR/FCR/DER を直書き）
- パレットインデックス(0–3) → RGB565 変換（DMG 風グリーンパレット）
- 160×144 を 240×320 の中央に配置（ウィンドウ転送）
- フレームバッファ FB（46080 バイト×2）を DTCM 上に確保（DMA からアクセス可能・非キャッシュ）
- SPI クロックは CCR 直書きで 33MHz（セクション 7 参照）。転送 約11ms でフレーム描画の裏に隠れる

**確定ピン配:**

| 信号 | Teensy 4.1 ピン | 備考 |
|---|---|---|
| SPI MOSI (LPSPI4) | 11 | |
| SPI MISO (LPSPI4) | 12 | |
| SPI SCK  (LPSPI4) | 13 | |
| CS       (LPSPI4) | 10 | |
| DC/RS | 9 | GPIO2 |
| RST | 8 | GPIO2 |
| BL | 7 | GPIO2（I2S 実装後は 6 番に変更予定） |

---

### D. CartridgeBus 実装 ✅ 完了（実機調整残）

ROM 供給は 2 経路あり、**現在 `main.rs` は `FlashCart`（Flash 埋め込み）を使用中**。

- **`teensy/src/sdcard.rs` `FlashCart`**（デフォルト）: `include_bytes!("../../roms/game.gb")`
  でビルド時に Flash へ埋め込む。RomOnly(32KB) と MBC1(最大 2MB ROM + 32KB RAM、バンク
  切替・外部 RAM 対応) をサポート。SDカード方式は将来実装予定（`embedded-sdmmc` 0.7）。
- **`teensy/src/cartridge.rs` `GpioCart`**: 実 GB カートリッジを GPIO バスで読む実装（下記）。
  現在 `main.rs` からは未使用（実機配線・A8–A15 確認待ち）。

**確定ピン配:**

| 信号 | Teensy ピン | GPIO ポート/ビット |
|---|---|---|
| D0–D7 | 14,15,40,41,17,16,22,23 | GPIO1[18–25] (連続) |
| /RD | 33 | GPIO4[7] |
| /WR | 34 | GPIO2[28] (t41 拡張) |
| A0–A7 | 19,18,38,39,26,27,0,1 | GPIO1[16–17,28–31,2–3] ※非連続 |
| A8–A15 | 拡張パッド | GPIO3/4 (TODO: 実機確認) |

**留意点:**
- GB は 5V 系 → 74AHCT245 等のレベルシフタが必要
- アクセスタイム: `cortex_m::asm::delay(90)` ≈ 150 ns @ 600 MHz
- A8–A15 のピン割り当ては TODO（要実機確認）

---

### E. I2S AudioSink 実装（優先度: 低）— スタブのみ

`teensy/src/audio.rs` に `PcmAudio` スタブを追加済み。
`AudioSink::push()` は現在 no-op。現在は `main.rs` で `NullAudio` を使用中。

**次のステップ:**
- `ccm_analog` で Audio PLL (PLL4) を 11.2896 MHz に設定
- CCM SAI1 clock root に割り当て
- `hal::sai::Sai::new()` + `SaiConfig::i2s(bclk_div(8))` で TX 初期化
- `push()` で f32 → i16 変換 + `sai_tx.write_frame(0, [l, r])`

**ピン配（SAI1 TX）:**

| 信号 | Teensy ピン | 備考 |
|---|---|---|
| SAI1_TX_DATA | 7 | BL と競合 → BL を pin 6 に変更要 |
| SAI1_TX_BCLK | 26 | |
| SAI1_TX_SYNC | 27 | |

参考: `imxrt-hal/examples/rtic_sai_pcm5102.rs`

---

### F. GPIO ボタン入力（優先度: 低）— スタブのみ

`teensy/src/input.rs` に `GpioInput` スタブを追加済み。
`InputSource::poll()` は現在 `ButtonState::default()`（全ボタン非押下）を返す。

**次のステップ:**
- 各ボタン用の GPIO ピンを確定（Teensy 4.1 の空きピン）
- `gpio_port.input(pin)` で各ピンを入力モードに設定
- `poll()` で PSR レジスタを読み `ButtonState` に変換

---

### G. その他

- **`teensy/` を Cargo.toml workspace から外す理由**: thumbv7em ターゲット専用のため、ホスト向け `cargo build` でエラーにならないよう `exclude` で別管理。ビルドコマンド: `cargo build -p gb-teensy --target thumbv7em-none-eabihf`
- **CI**: `cargo build -p gb-core --target thumbv7em-none-eabihf` をコア健全性チェックとして追加推奨
- **BootROM**: Teensy では `include_bytes!("dmg_bootrom.bin")` で Flash に埋め込む（著作権に注意 — 配布不可）

---

## ファイルマップ（現在）

```
core/src/
├── lib.rs          #![cfg_attr(not(test), no_std)]
├── platform.rs     Display / AudioSink / CartridgeBus トレイト + Null* 実装
├── input.rs        ButtonState / InputSource トレイト
├── bootrom.rs      from_bytes() / disabled() — ファイルI/O不使用
├── mmu.rs          Mmu<C: CartridgeBus> + MemoryBus trait + TestHarness(feature gate)
├── gameboy.rs      GameBoy<C,D,A,I> + step() + StepResult
├── cpu.rs          + cpu/{decode,exec,instr,operand,registers}.rs
├── ppu.rs          heapless::Vec<SpriteData,10>, [u8;N] 配列
├── apu.rs          emulate_cycle() -> Option<(f32,f32)>
├── timer.rs
├── joypad.rs
├── hram.rs
└── wram.rs

host/src/
├── main.rs         wall-clock catch-up ループ / run_headless()
├── lcd.rs          SdlDisplay / SdlAudio / SdlInput + create_sdl_backends()
├── cartridge.rs    MBC エミュ + impl CartridgeBus for Cartridge
└── renderer.rs     TerminalRenderer（未使用・保留）

teensy/src/
├── main.rs         エントリ / cache有効化 / SPI 33MHz / FlashCart / ペーシングループ
├── display/
│   ├── mod.rs      DmaDisplay<P,SPI,DC,RST> — impl Display ✅ (eDMA + ダブルバッファ)
│   └── panel.rs    PanelController / Ili9341 (パネル抽象)
├── sdcard.rs       FlashCart — impl CartridgeBus ✅ (Flash 埋め込み, RomOnly/MBC1)
├── cartridge.rs    GpioCart — impl CartridgeBus ✅ (実カート用 / A8-A15 TODO・未使用)
├── audio.rs        PcmAudio — impl AudioSink (スタブ / SAI 未実装)
└── input.rs        GpioInput — impl InputSource (スタブ / ピン未割当)
```

## タスク一覧

| # | タスク | 状態 |
|---|---|---|
| A | `cargo run -p gb-host` で実機 PC 動作確認 | 未着手 |
| B | `teensy/` クレートの骨組み（imxrt-rt 化済み） | ✅ 完了 |
| C | ILI9341 Display 実装（DMA 描画 + パネル抽象） | ✅ 完了 |
| D | CartridgeBus 実装（FlashCart 使用中 / GpioCart 実カート用） | ✅ 完了（A8-A15 ピン確認待ち） |
| E | I2S AudioSink 実装 | スタブのみ |
| F | GPIO ボタン入力実装 | スタブのみ |
| G | CI / その他 | 未着手 |
| H | Teensy 実機の性能最適化（cache / SPI 33MHz / ペーシング） | ✅ 完了（59.7fps 固定） |
