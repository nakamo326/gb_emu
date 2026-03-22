# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**重要: このプロジェクトでは日本語で回答してください。**

## Git コミット方針

コミットは意味のある最小単位で行う。1つのコミットに複数の独立した変更を混在させない。

## ビルドと実行

```bash
cargo build
cargo run
cargo build --release
cargo test
cargo check
```

実行には作業ディレクトリに `dmg_bootrom.bin` が必要。起動時に `test_rom.gb` または `cpu_instrs.gb` があれば自動ロードする。

## アーキテクチャ

### メインループ（`src/gameboy.rs`）

`GameBoy` 構造体がCPU・MMU・レンダラーを統括。ループはナノ秒精度でM-cycleタイミング（954ns/cycle）に同期し、毎サイクルCPUとPPUを1ステップ進める。PPUがフレーム完成を通知したときのみ `Renderer::draw()` を呼ぶ。

### CPU（`src/cpu.rs` + `src/cpu/`）

- `Cpu::emulate_cycle()` → `decode()` の順で実行
- `Ctx` 構造体がデコード中のオペコードと CB プレフィックスフラグを保持
- サブモジュール: `decode.rs`（デコード）、`instructions.rs`（命令実装）、`operand.rs`、`registers.rs`

### MMU（`src/mmu.rs`）

アドレスデコードして各コンポーネントに委譲：

| アドレス | コンポーネント |
|---|---|
| 0x0000–0x00FF | Bootrom（アクティブ時）or カートリッジ |
| 0x0100–0x7FFF, 0xA000–0xBFFF | カートリッジ（MBC経由） |
| 0x8000–0x9FFF, 0xFE00–0xFE9F, 0xFF40–0xFF4B | PPU |
| 0xC000–0xFDFF | WRAM |
| 0xFF50 | Bootrom無効化レジスタ |
| 0xFF80–0xFFFE | HRAM |

### カートリッジ / MBC（`src/cartridge.rs`）

`MemoryBankController` トレイトで抽象化。実装済み: `RomOnly`、`Mbc1`。未対応MBC（MBC3、MBC5等）は暫定的に `RomOnly` にフォールバック。

### PPU（`src/ppu.rs`）

モード遷移: OAMScan → Drawing → HBlank → (144行で) VBlank。160×144ピクセルバッファ（パレットインデックス0–3）を出力。`emulate_cycle()` がフレーム完成時に `true` を返す。

### レンダリング（`src/renderer.rs`、`src/lcd.rs`）

`Renderer` トレイト（`draw(&[u8])`）を実装：
- **`Lcd`**（`lcd.rs`）: SDL2でウィンドウ表示。スケール4倍。デフォルト使用。
- **`TerminalRenderer`**（`renderer.rs`）: ASCIIアートでターミナル出力。`main.rs` でコメントアウト中。

SDL2依存: macOSは `raw-window-handle` 機能、その他は静的リンク（`bundled`）を使用（`Cargo.toml` 参照）。
