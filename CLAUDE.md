# CLAUDE.md

このファイルは、このリポジトリでコードを扱う際のClaude Code (claude.ai/code) へのガイダンスを提供します。

**重要: このプロジェクトでは日本語で回答してください。**

## プロジェクト概要

これはRustで書かれたゲームボーイエミュレータで、オリジナルのゲームボーイハードウェアのコアコンポーネントを実装しています。エミュレータはゲームボーイの4.194304 MHzのCPUクロック周波数に従い、M-cycle（4クロックサイクルずつ）で本格的なタイミングを実装しています。

## ビルドシステム

標準的なRust/Cargoプロジェクト:

```bash
# プロジェクトをビルド
cargo build

# エミュレータを実行
cargo run

# 最適化されたリリース版をビルド
cargo build --release

# テストを実行
cargo test

# ビルドせずにコードをチェック
cargo check
```

## コアアーキテクチャ

### メインコンポーネント

- **GameBoy** (`src/gameboy.rs`): CPU、MMU、レンダリングを本格的なタイミングで統括するメインエミュレータ構造体
- **CPU** (`src/cpu.rs`): 命令デコード/実行サイクルを持つZ80ライクプロセッサ実装
- **MMU** (`src/mmu.rs`): メモリマップドI/Oとアドレスデコーディングを処理するメモリ管理ユニット
- **PPU** (`src/ppu.rs`): ゲームボーイのグラフィックスレンダリングパイプラインを実装するピクチャープロセッシングユニット
- **Renderer** (`src/renderer.rs`): ターミナル出力実装を含むトレイトベースのレンダリングシステム

### メモリコンポーネント

- **Bootrom** (`src/bootrom.rs`): ブートROM実装（`dmg_bootrom.bin`から読み込み）
- **WRAM** (`src/wram.rs`): ワーキングRAM（0xC000-0xFDFF）
- **HRAM** (`src/hram.rs`): 高速RAM（0xFF80-0xFFFE）

### CPU実装

CPUは個別のモジュールを持つモジュラー設計を使用:
- `cpu/decode.rs`: 命令デコーディングロジック
- `cpu/instructions.rs`: 命令実装
- `cpu/operand.rs`: オペランド処理
- `cpu/registers.rs`: レジスタファイル実装

### メモリマップ

MMUは適切なアドレスデコーディングでゲームボーイのメモリマップを実装:
- 0x0000-0x00FF: ブートROM（アクティブ時）
- 0x8000-0x9FFF: VRAM（PPU）
- 0xC000-0xFDFF: WRAM
- 0xFE00-0xFE9F: OAM（PPU）
- 0xFF40-0xFF4B: PPUレジスタ
- 0xFF50: ブートROM無効化レジスタ
- 0xFF80-0xFFFE: HRAM

### PPU実装

PPUはゲームボーイのグラフィックスパイプラインを実装:
- 本格的なPPUモード（HBlank、VBlank、OAMScan、Drawing）
- 各モードの適切なタイミングサイクル
- タイルマップとタイルデータを使用した背景レンダリング
- 異なるモード中のメモリアクセス制限
- 160x144ピクセルバッファ出力

### レンダリングシステム

複数のレンダラー実装を可能にするトレイトベース設計:
- `TerminalRenderer`: ピクセルにASCII文字を使用してターミナルに出力
- Rendererトレイトにより、SDL2、OpenGL、その他のバックエンドの追加が容易

## 開発ノート

### タイミング

エミュレータは本格的なゲームボーイタイミングを実装:
- CPUは4.194304 MHzで動作
- M-cycleは4クロックサイクル
- メインループはナノ秒精度でリアルタイムと同期

### 依存関係

- SDL2（Cargo.tomlでプラットフォーム固有の設定）
- macOS: raw-window-handle機能を使用
- その他のプラットフォーム: バンドルされた静的リンクを使用

### プロジェクト構造

```
src/
├── main.rs           # エントリーポイント
├── gameboy.rs        # メインエミュレータ統括
├── cpu.rs            # CPU実装
├── cpu/              # CPUサブモジュール
├── mmu.rs            # メモリ管理
├── ppu.rs            # グラフィックス処理
├── renderer.rs       # レンダリング抽象化
└── [メモリモジュール]  # bootrom.rs, wram.rs, hram.rs
```

### テスト

`cargo test`でテストを実行します。プロジェクト構造は個々のコンポーネントのユニットテストをサポートしています。