# Teensy 4.1 実機動作手順書

作成日: 2026-06-11

---

## 概要

**Game Boy エミュレータを Teensy 4.1 + ILI9341 ディスプレイで動かす**手順書です。

カートリッジ読み込み回路は未作成のため、ROM を Teensy の Flash に書き込む方式を採用しています（Flash は 8MB あるので 512KB 以下のゲームは余裕で入ります）。

将来的な SDカード対応については末尾の「今後の拡張」を参照してください。

---

## 必要なもの

### ハードウェア

| 品目 | 仕様 |
|---|---|
| Teensy 4.1 | 購入済み |
| ILI9341 ディスプレイ | 2.4 or 2.8 インチ, SPI 接続, 240×320 |
| USB-A to Micro-B ケーブル | 書き込み・給電用 |
| ジャンパーワイヤー | |

### ソフトウェア

- Rust (rustup 経由でインストール済み想定)
- `cargo-binutils` (HEX ファイル生成に必要)
- `teensy_loader_cli` (書き込みツール)

---

## 1. 開発環境セットアップ

### 1-1. Rust ターゲットの追加

```bash
# ARM Cortex-M7 ターゲットを追加
rustup target add thumbv7em-none-eabihf

# cargo objcopy コマンドのインストール
cargo install cargo-binutils
rustup component add llvm-tools-preview
```

### 1-2. Teensy Loader のインストール

**Linux (Ubuntu/Debian):**
```bash
# パッケージマネージャから
sudo apt install teensy-loader-cli

# または公式サイトからバイナリをダウンロード
# https://www.pjrc.com/teensy/loader_cli.html
```

**macOS:**
```bash
brew install teensy_loader_cli
```

**Windows:**
Teensy Loader GUI を公式サイトからダウンロード:
https://www.pjrc.com/teensy/loader.html

---

## 2. ディスプレイ配線 (ILI9341)

ILI9341 と Teensy 4.1 をジャンパーワイヤーで接続します。

```
ILI9341 ピン    Teensy 4.1 ピン    備考
────────────────────────────────────────────
VCC        →    3.3V              ※ 5V 接続禁止
GND        →    GND
CS         →    10               SPI チップセレクト
RESET      →    8                GPIO
DC/RS      →    9                コマンド/データ切り替え
SDI/MOSI   →    11               SPI データ入力
SCK/CLK    →    13               SPI クロック
LED/BL     →    7                バックライト (330Ω 抵抗を直列に入れると安全)
SDO/MISO   →    12               SPI データ出力 (省略可)
```

> **重要**: VCC は必ず **3.3V** に接続してください。Teensy 4.1 は 3.3V 系です。

---

## 3. ROM の準備

ビルド時に ROM を Flash へ埋め込みます。

リポジトリの `roms/` ディレクトリに **`game.gb`** という名前で ROM ファイルを置きます。

```bash
# 例: KIRBY2.gb を使う場合
cp /path/to/your/rom.gb /path/to/gb_emu/roms/game.gb
```

> **著作権について**: 所有しているゲームの ROM を個人利用の範囲でのみ使用してください。ROM ファイルをリポジトリにコミットしないよう注意してください (`.gitignore` に追加推奨)。

---

## 4. ビルド

```bash
cd /path/to/gb_emu/teensy

# デバッグビルド (初回確認用)
cargo build --target thumbv7em-none-eabihf

# リリースビルド (実機実行用 — LTO 有効で最適化済み)
cargo build --target thumbv7em-none-eabihf --release
```

ビルド成功時の出力例:
```
Finished `release` profile [optimized + debuginfo] target(s) in XX.XXs
```

### 4-1. ELF → HEX 変換

Teensy Loader は HEX ファイルを必要とします。

```bash
# デバッグ版
cargo objcopy --target thumbv7em-none-eabihf -- \
  -O ihex ../target/thumbv7em-none-eabihf/debug/gb-teensy.hex

# リリース版 (こちらを使用推奨)
cargo objcopy --target thumbv7em-none-eabihf --release -- \
  -O ihex ../target/thumbv7em-none-eabihf/release/gb-teensy.hex
```

---

## 5. Teensy への書き込み

### Teensy Loader CLI を使う場合

```bash
# Teensy 4.1 を USB 接続した状態で、Teensy 本体のプログラムボタン (小さいボタン) を押す
# その後すぐに以下を実行:
teensy_loader_cli --mcu=TEENSY41 -w -v \
  ../target/thumbv7em-none-eabihf/release/gb-teensy.hex
```

フラグの説明:
- `--mcu=TEENSY41`: Teensy 4.1 を指定
- `-w`: プログラムボタンが押されるまで待機
- `-v`: 書き込み後に自動リセット

### Teensy Loader GUI を使う場合 (Windows)

1. Teensy Loader を起動
2. HEX ファイルをウィンドウにドラッグ&ドロップ
3. Teensy のプログラムボタンを押す
4. 自動書き込み開始 → 書き込み完了後に自動リセット

---

## 6. 動作確認

書き込み完了後、Teensy が自動的に再起動します。

### 正常動作のチェックリスト

```
[ ] ILI9341 ディスプレイのバックライトが点灯する
[ ] 数秒以内にゲーム画面が表示される
[ ] BootROM スキップ時は Nintendo ロゴなしで直接ゲームが起動する
[ ] フレームが更新され続ける (約 60fps)
```

### うまくいかない場合

**ディスプレイが真っ暗:**
- バックライト (BL/LED ピン → Teensy pin 7) の配線確認
- RST ピン (pin 8) の配線確認
- SPI の MOSI/SCK ピン (pin 11, 13) の配線確認

**ビルドエラー `include_bytes!` でファイルが見つからない:**
```
error: couldn't read ../../roms/game.gb: No such file or directory
```
→ `roms/game.gb` が存在するか確認する

**書き込み後に何も起きない:**
- HEX ファイルのパスが正しいか確認
- Teensy 4.1 のプログラムボタンを押した後に書き込みコマンドを実行しているか確認

---

## 7. ROM の変更方法

別のゲームを試す場合は、`roms/game.gb` を差し替えて再ビルド・再書き込みするだけです。

```bash
# 別の ROM に差し替え
cp /path/to/another_game.gb /path/to/gb_emu/roms/game.gb

# 再ビルド
cargo build --target thumbv7em-none-eabihf --release

# HEX 変換
cargo objcopy --target thumbv7em-none-eabihf --release -- \
  -O ihex ../target/thumbv7em-none-eabihf/release/gb-teensy.hex

# 書き込み
teensy_loader_cli --mcu=TEENSY41 -w -v \
  ../target/thumbv7em-none-eabihf/release/gb-teensy.hex
```

---

## 8. 現在の実装状況

| 機能 | 状態 | 備考 |
|---|---|---|
| ディスプレイ (ILI9341) | ✅ 実装済み | RGB565, 160×144 を中央に配置 |
| ROM 読み込み (Flash 埋め込み) | ✅ 実装済み | `roms/game.gb` をビルド時に埋め込み |
| MBC なし (ROM Only, 32KB) | ✅ 対応 | |
| MBC1 (最大 512KB) | ✅ 対応 | |
| BootROM | 無効化済み | `Bootrom::disabled()` で Nintendo ロゴをスキップ |
| サウンド (I2S) | ❌ 未実装 | 無音で動作 |
| ボタン入力 | ❌ 未実装 | 全ボタン非押下状態 (デモのみ) |
| MBC3/MBC5 | ❌ 未対応 | 対象 ROM は起動しない |

---

## 9. 今後の拡張

### SDカードから ROM を読み込む (将来対応)

現在 `teensy4-bsp 0.4` は `embedded-hal 0.2` を使用しており、
`embedded-sdmmc` 最新版 (0.7+, embedded-hal 1.0 必須) と非互換です。

以下のいずれかで解決後に対応予定:
- `teensy4-bsp` が embedded-hal 1.0 対応版 (0.5+) にアップデートされる
- `embedded-sdmmc 0.3` (embedded-hal 0.2 互換) を使って実装する

SDカードを使う場合のハードウェア構成案:

```
SDカードモジュール (SPI)   Teensy 4.1 ピン
──────────────────────────────────────────
MOSI                 →    11  (LPSPI4 MOSI, ディスプレイと共有)
MISO                 →    12  (LPSPI4 MISO, ディスプレイと共有)
SCK                  →    13  (LPSPI4 SCK, ディスプレイと共有)
CS                   →    5   (独立 GPIO — ディスプレイ CS=10 とは別)
VCC                  →    3.3V
GND                  →    GND
```

LPSPI4 の SPI バスをディスプレイと共有し、CS ピンを分けることで 1 つの SPI バスで両デバイスを制御できます。

### ボタン入力 (`teensy/src/input.rs`)

`GpioInput` の `poll()` を実装してゲームを操作できるようにする。空きピンを 8 個用意してボタンを接続する。

### サウンド (`teensy/src/audio.rs`)

SAI1 (I2S) + PCM5102A DAC で音声出力を実装する。
詳細は [PORTING_STATUS.md](../PORTING_STATUS.md) のタスク E を参照。

### GB カートリッジの直接読み込み (`teensy/src/cartridge.rs`)

74AHCT245 レベルシフタ経由で実 GB カートリッジのバスに接続する。
詳細は [PORTING_STATUS.md](../PORTING_STATUS.md) のタスク D を参照。
