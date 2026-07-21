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
CS         →    GND               常時選択 (実機検証済みの確実な接続)
RESET      →    3.3V              リセット解除に固定 (実機検証済み)
DC/RS      →    9                コマンド/データ切り替え
SDI/MOSI   →    11               SPI データ入力 (※ SDO と間違えない)
SCK/CLK    →    13               SPI クロック
LED/BL     →    3.3V              バックライト (※下記の重要事項を参照)
SDO/MISO   →    12               SPI データ出力 (省略可)
```

> **重要 1 — VCC**: 必ず **3.3V** に接続してください。Teensy 4.1 は 3.3V 系です。
> 配線時は「どの 3.3V ピンか」を必ず確認すること (挿し間違いで全く動かない事例あり)。
>
> **重要 2 — バックライト (LED/BL)**: **Teensy の GPIO ピンでは電流が不足してバックライトを
> 駆動できません**。LED/BL は **3.3V に直結** してください (常時点灯)。GPIO で ON/OFF 制御
> したい場合はトランジスタ等の外部回路が必要です。
>
> **重要 3 — CS / RESET**: 上表では CS を GND、RESET を 3.3V に固定しています (単一 SPI
> デバイスで最も確実、実機で検証済み)。Teensy 側で制御したい場合は CS→10 (ハードウェア
> PCS0)、RESET→8 に変更できますが、その場合はソフト側のピン割り当てと整合させること。
>
> **重要 4 — SPI 配線の接触**: ブレッドボード + ジャンパ線は接触不良・断線が起きやすい。
> 「コマンドが全く効かず画面が真っ白」のときは SCK(13)/SDI(11) の線を交換・挿し直す。

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
- バックライト (BL/LED ピン) が 3.3V に直結されているか確認（GPIO 駆動は電流不足で不可）
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
| サウンド (I2S) | ⚠️ 実装済みだが無効化中 | 有効化すると画面が黒くなる問題あり。[既知の問題](#11-既知の問題) 参照 |
| ボタン入力 (2x4 マトリクス) | ✅ 実装済み | 配線は [teensy_button_wiring.md](teensy_button_wiring.md) 参照 |
| MBC3/MBC5 | ❌ 未対応 | 対象 ROM は起動しない |

---

## 9. 今後の拡張

### SDカードから ROM を読み込む (将来対応)

`teensy4-bsp 0.5` / `imxrt-hal 0.5` は embedded-hal 0.2 と 1.0 の両方を実装するため、
`embedded-sdmmc` 最新版 (0.7+, embedded-hal 1.0 必須) と組み合わせて実装できます。
`board::lpspi(...)` の戻り値を embedded-hal 1.0 の `SpiDevice` を満たすようラップして渡します。

> **メモリの注意**: SD から読んだ ROM を置く RAM バッファは `.bss`/`.uninit` に確保しますが、
> フル ROM (MBC5 で最大 8MB) はオンチップ RAM (OCRAM 512KB / DTCM 320KB) に収まりません。
> 「小さい ROM 限定」か「バンク単位のオンデマンド読み込み」が前提になります。
> あわせて後述の「rodata のメモリ配置」も参照してください。

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

### サウンド (`teensy/src/audio.rs`)

SAI1 (I2S) + MAX98357A/PCM5102A DAC 向けの実装自体は完了しているが、有効化すると画面が
黒くなる問題が未解決のため `main.rs` では `NullAudio` にフォールバックしている。
詳細は [11. 既知の問題](#11-既知の問題) と [PORTING_STATUS.md](../PORTING_STATUS.md) のタスク E を参照。

### GB カートリッジの直接読み込み (`teensy/src/cartridge.rs`)

74AHCT245 レベルシフタ経由で実 GB カートリッジのバスに接続する。
GPIO バスから都度読み出すため ROM バッファは不要で、RAM を圧迫しない。
詳細は [PORTING_STATUS.md](../PORTING_STATUS.md) のタスク D を参照。

---

## 10. rodata のメモリ配置 (`teensy/build.rs`)

ランタイムは imxrt-rt の `RuntimeBuilder` (`teensy/build.rs`) でメモリ配置を決めている。
現在 `.rodata(Memory::Flash)` を明示指定しているが、これは **ROM の入手方法に依存した判断**である。

| ROM の入手方法 | ROM の置き場所 | `.rodata` の中身 | 推奨配置 |
|---|---|---|---|
| **現在**: `include_bytes!` で埋め込み | `.rodata` (最大数 MB) | ROM 全体 + 定数 | **Flash 必須** |
| SDカード読み込み | 実行時 RAM バッファ (`.bss`/`.uninit`) | パレット・ILI9341 初期化表等の小定数のみ | OCRAM (デフォルト) |
| 実カートリッジ直読み (GpioCart) | バッファ不要 (GPIO バス直読み) | 同上、小定数のみ | OCRAM (デフォルト) |

- **なぜ今 Flash か**: `include_bytes!` で埋め込んだ ROM は `.rodata` に入り、MBC1/3/5 で最大数 MB になり得る。
  imxrt-rt のデフォルト配置先 OCRAM (512KB) には収まらないため、RAM にコピーせず Flash 上 (XIP) に置いている。
- **将来 OCRAM に戻す条件**: ROM を SDカードや実カートリッジから読むようになり `include_bytes!` をやめたら、
  `.rodata` は KB 級の小定数だけになる。そのときは `build.rs` の `.rodata(Memory::Flash)` 行を削除し、
  imxrt-rt のデフォルト (OCRAM) に戻すと、外部 FlexSPI flash の XIP 読み出しより速いオンチップ RAM 読み出しになる。

---

## 11. 既知の問題

### SAI1 オーディオ有効化時に画面が真っ黒になる (未解決, 2026-07-05)

`teensy/src/audio.rs` の `SaiAudio` (SAI1 + MAX98357A/PCM5102A 向け I2S 出力) を有効にすると、
起動から数秒〜数十秒のランダムなタイミングでディスプレイが真っ黒になる。音声出力自体は
黒画面になった後も継続する。**現在 `main.rs` では `SaiAudio` を使わず `NullAudio` にフォール
バックして無効化している。**

#### 症状の詳細

- 画面は GB のゲーム画面だけでなく、FPS オーバーレイ文字も含めて完全に真っ黒になる
  (パネルへの描画コマンドが一切反映されていない状態)
- USB シリアルログ (`imxrt-log`) 上は FPS ~59.7・drops:0 で正常に動作し続けており、
  パニックやクラッシュは一切発生していない (`gb.step()` / `draw()` は毎フレーム正常に呼ばれている)
- 黒くなるタイミングは毎回異なり、起動直後のこともあれば数十秒後のこともある

#### 切り分け済み (原因ではないと確認したもの)

| 仮説 | 検証方法 | 結果 |
|---|---|---|
| MAX98357A のスピーカー駆動電流によるレール電圧のブラウンアウト | アンプをブレッドボードから完全に撤去 | 改善せず |
| pin23 (SAI1_MCLK) と物理カートリッジ回路 (D7) のバス競合 | カートリッジ回路自体が未接続であることを確認 | 該当せず |
| SAI1 の配線とディスプレイ配線間のクロストーク | 配線をやり直し | 改善せず |
| `main()` 内の初期化順序 (SAI1 init → Display init の順序依存) | Display init の後に SAI1 init するよう順序を入れ替え | 改善せず |
| 電源/GND経由のノイズによる ST7789 RST ラインへのグリッチ | ディスプレイモジュールの VCC-GND 直近にバイパスコンデンサ (0.1µF セラミック + 数十µF 電解) を追加 | 改善せず (2026-07-08) |

これらに加え、黒くなるタイミングが毎回一定でない (決定的な初期化順序バグなら再現タイミングは
揃うはず) ことから、単純な初期化順序のバグや電源ノイズよりも、SAI1 の割り込み
(`FIFO_REQUEST`, 数kHz 相当で発生) と `display.rs` の DMA 制御 (特に生レジスタ操作の
`finalize_continuous_transfer()`) との間の低頻度なタイミング競合が濃厚と見ている
(バイパスコンデンサでの改善なしにより電源ノイズ説は棄却)。未確定。

#### 未検証・次の一手候補

- ロジックアナライザ/オシロで ST7789 の RST ピンを監視し、黒画面になる瞬間に実際にパルスが
  入っていないか確認する

#### 試して切り戻したもの

| 仮説 | 検証方法 | 結果 |
|---|---|---|
| SAI1 割り込みと `display.rs` の DMA 制御の時間的競合 | `draw()` 実行中 (`wait_dma_complete()`〜`start_dma()`) に `NVIC::mask(SAI1)`/`unmask(SAI1)` で排他化 | 音声が電源投入時の一瞬 (beep) 以降まったく出なくなる副作用が発生したため切り戻し確定 (2026-07-09)。マスク除去後は音声が (途切れ途切れながら) 再生されることを確認済み。原因未特定 (FIFO_REQUEST がマスク解除後に想定通り再発火していない可能性など)。この手法は不採用とし、黒画面バグは別の切り分け方法 (RSTピン監視など) を検討する |

#### 再現条件

- ROM: 任意 (`ZELDA_DIN.gb` 等の通常プレイで再現)
- `SaiAudio::new(...)` を呼び、`NVIC::unmask(bsp::interrupt::SAI1)` した時点で発生する
  (アンプ・スピーカーの接続有無、カートリッジ回路の有無は無関係)

> **注記 (2026-07-09)**: 下記「音声が途切れ途切れになる」の調査で、1 秒に 1 回
> (FPS/overlay 更新のタイミング) 数百ms 規模のフレーム処理遅延が発生していることが判明した。
> 当初 `log::info!` のブロッキングを疑ったが実機再検証で棄却済み (`imxrt-log` は非ブロッキング
> 設計)。真因は未確定 (`render_overlay()` の PIO SPI 書き込み自体を疑い検証中)。本項の
> 「USB シリアルログ上は FPS ~59.7・drops:0 で正常」という過去の切り分け結果も、この 1秒に1回
> の遅延の実体が確定するまでは再検証の余地がある。

### 音声が途切れ途切れ・全体的に遅い/ピッチが低い (調査中, 2026-07-09)

SAI1 オーディオ有効化後、画面の黒化とは別に「音が途切れ途切れになる」「全体的に音楽が
遅れている (ピッチが低い)」という 2 つの症状が確認された。後者は、フレーム処理が
一時的に大きく遅延した際に Teensy のフレームペーシングが「遅延を取り戻さずその場で
再同期する」設計 (`main.rs` のコメント参照) であるため、遅延が毎秒発生し続けると
実時間との同期が累積的にずれていくことが原因と推測している (両症状は同一原因の
可能性が高い)。

#### 切り分け済み

| 仮説 | 検証方法 | 結果 |
|---|---|---|
| `display.rs` の `log::info!("FPS:...")` / `log::info!("overlay:...")` が USB シリアル送出でブロックし、その間 `gb.step()` (APU サンプル生成・`audio.push()` 含む) が止まる | シリアルログを実機で採取したところ、ログ出力と同じタイミング (1秒に1回) で `peak:` が 800〜1287% (通常 50% 前後) に跳ね上がり `drops:1` を記録。該当 `log::info!` 呼び出しを `display/mod.rs` の `update_fps()` / `draw()` から削除 | 実機再検証の結果、途切れ・全体的な遅さとも改善せず。棄却 (2026-07-09)。`imxrt-log` は `poll()`/producer 側とも非ブロッキング設計 (ソース確認済み) であり、そもそも log 呼び出し自体が長時間ブロックする実装ではなかった |
| `render_overlay()` の PIO (ブロッキング) SPI 書き込み自体 (ログとは別に、画面右上への FPS/負荷テキスト描画処理そのもの) が 1 秒に 1 回 (`fps_dirty` のタイミング) 長時間の遅延を引き起こしている | `draw()` から `render_overlay()` の呼び出し自体を無効化 (計算・`fps_dirty` 判定は残し、描画呼び出しのみ削除) | 実機再検証待ち |

現状: `update_fps()` 内の FPS ログ、`draw()` 内の overlay 描画時間ログ (前段の仮説) を削除済み。
さらに `draw()` から `render_overlay()` の呼び出し自体も無効化 (今回)。オーバーレイ機能
(FPS/負荷の画面表示) は一時的に見えなくなる。
