# Teensy 4.1 移植 — 現状と残タスク

作成日: 2026-06-10  
最終更新: 2026-06-11

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
├── .cargo/
│   └── config.toml          # target = thumbv7em-none-eabihf, runner
├── memory.x                 # i.MX RT1062 リンカスクリプト
├── build.rs                 # memory.x を rustc に渡す
└── src/
    ├── main.rs              # #![no_std] #![no_main] + エントリポイント
    ├── display.rs           # ILI9341 ドライバ
    ├── cartridge.rs         # GPIO カートリッジバス
    ├── audio.rs             # I2S オーディオ (スタブ)
    └── input.rs             # GPIO ボタン入力 (スタブ)
```

`cargo build -p gb-teensy --target thumbv7em-none-eabihf` でビルド可能。

---

### C. ILI9341 Display 実装 ✅ 完了

`teensy/src/display.rs` に `Ili9341Display<SPI, DC, RST>` として実装済み。

**実装内容:**
- `embedded-hal` 0.2 SPI トレイトで手書き ILI9341 ドライバ
- パレットインデックス(0–3) → RGB565 変換テーブル（DMG 風グリーンパレット）
- 160×144 を 240×320 の中央に配置（ウィンドウ転送）
- 46080 バイトのフレームバッファを DTCM 上に確保

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

### D. GPIO CartridgeBus 実装 ✅ 完了（実機調整残）

`teensy/src/cartridge.rs` に `GpioCart` として実装済み。

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
├── main.rs         エントリポイント / ピン初期化 / GameBoy ループ
├── display.rs      Ili9341Display<SPI,DC,RST> — impl Display ✅
├── cartridge.rs    GpioCart — impl CartridgeBus ✅ (A8-A15 TODO)
├── audio.rs        PcmAudio — impl AudioSink (スタブ / SAI 未実装)
└── input.rs        GpioInput — impl InputSource (スタブ / ピン未割当)
```

## タスク一覧

| # | タスク | 状態 |
|---|---|---|
| A | `cargo run -p gb-host` で実機 PC 動作確認 | 未着手 |
| B | `teensy/` クレートの骨組み | ✅ 完了 |
| C | ILI9341 Display 実装 | ✅ 完了 |
| D | GPIO CartridgeBus 実装 | ✅ 完了（A8-A15 ピン確認待ち） |
| E | I2S AudioSink 実装 | スタブのみ |
| F | GPIO ボタン入力実装 | スタブのみ |
| G | CI / その他 | 未着手 |
