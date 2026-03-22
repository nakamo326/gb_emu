# Game Boy エミュレーター 実装状況

最終更新: 2026-03-23

## 実装状況一覧

| 機能 | 状態 | 備考 |
|------|------|------|
| CPU 全命令 | ✅ 完了 | CB プレフィックス含む全512命令 |
| 割り込み | ✅ 完了 | VBlank/STAT/Timer/Joypad/Serial |
| タイマー | ✅ 完了 | DIV/TIMA/TMA/TAC |
| BG 描画 | ✅ 完了 | SCX/SCY スクロール・BGP パレット |
| ウィンドウ描画 | ✅ 完了 | WX/WY・WINDOW_TILE_MAP 対応 |
| スプライト描画 | ✅ 完了 | OAMScan・8x16モード・OBP0/OBP1・優先度 |
| ジョイパッド入力 | ✅ 完了 | SDL2 キーマッピング・割り込み生成 |
| MBC1 | ✅ 完了 | ROM/RAM バンク切り替え |
| MBC3 | ✅ 完了 | バンク切り替え（RTC は未実装） |
| MBC5 | ✅ 完了 | 9ビット ROM バンク・4ビット RAM バンク |
| OAM DMA | ✅ 完了 | 0xFF46 書き込みで 160 バイト転送 |
| APU（音声） | ✅ 完了 | CH1–4・Frame Sequencer・SDL2 AudioQueue |
| blargg cpu_instrs | ✅ 全 pass | 全11テスト |
| MBC3 RTC | ❌ 未実装 | ポケモン金銀等の時計機能 |
| シリアル通信 | ⚠️ 最小限 | テスト ROM 出力のみ・転送タイミング未実装 |

## 各機能の詳細

### ✅ CPU（`src/cpu.rs` + `src/cpu/`）

- 全命令実装済み（CB プレフィックス含む 512 命令）
- HALT バグ・EI ディレイのクセ（quirk）対応
- blargg `cpu_instrs` テスト全 pass

### ✅ 割り込み（`src/cpu.rs`）

- VBlank / LCD STAT / Timer / Joypad / Serial 割り込み対応
- IME / IE / IF レジスタ実装済み
- EI 命令後の 1 命令遅延も正確に実装

### ✅ タイマー（`src/timer.rs`）

- DIV / TIMA / TMA / TAC 実装済み
- M-cycle 精度のカウンタ

### ✅ BG 描画 / ウィンドウ描画 / スプライト描画（`src/ppu.rs`）

- タイルマップ・タイルデータ描画（アドレッシングモード両対応）
- SCX / SCY スクロール、BGP パレット
- WX / WY ウィンドウ描画（内部 Y カウンタで正確な行追跡）
- OAMScan でスプライト評価（最大 10 個）
- 8x8 / 8x16 スプライトモード、X/Y フリップ、OBP0/OBP1 パレット
- BG-スプライト優先度処理
- OAMScan / Drawing / HBlank / VBlank モード遷移
- LYC=LY 割り込み・STAT 割り込み

### ✅ ジョイパッド入力（`src/joypad.rs`）

| ゲームボタン | キー |
|---|---|
| A | Z |
| B | X |
| Start | Return |
| Select | Right Shift |
| 十字キー | 矢印キー |

### ✅ カートリッジ / MBC（`src/cartridge.rs`）

- RomOnly / MBC1 / MBC3 / MBC5 実装済み
- MBC3 RTC レジスタはスタブ（読み出しは 0x00）

### ✅ APU（音声）（`src/apu.rs`）

4チャンネル構成 + マスター制御をフルに実装。

| チャンネル | 種別 | レジスタ | 状態 |
|---|---|---|---|
| CH1 | 矩形波 + Sweep | 0xFF10–0xFF14 | ✅ |
| CH2 | 矩形波 | 0xFF16–0xFF19 | ✅ |
| CH3 | Wave RAM 再生 | 0xFF1A–0xFF1E, 0xFF30–0xFF3F | ✅ |
| CH4 | LFSR ノイズ | 0xFF20–0xFF23 | ✅ |

- Frame Sequencer（512 Hz / 2048 M-cycle）で Length・Envelope・Sweep をクロック
- NR50（マスターボリューム）/ NR51（パンニング）/ NR52（電源）実装済み
- NR52 電源 OFF 時にレジスタをリセット、Wave RAM は保持
- 分数カウンタ方式で 44100 Hz サンプリング
- SDL2 AudioQueue（ステレオ f32）で出力
- オーディオデバイス不在時は警告を出して無音で継続（WSL2 対応）

**PulseAudio（WSLg）での音声出力:**
WSL2 では `libpulse-dev` をインストールして SDL2 を再ビルドすることで PulseAudio 経由で音声が出る。

```bash
sudo apt install libpulse-dev
cargo clean && cargo build
```

### ❌ MBC3 RTC

- 0x08–0x0C の RTC レジスタは常に 0x00 を返す
- ラッチ機構（0x6000–0x7FFF）未実装
- ポケモン金銀など時間連動ゲームで時計が動かない

### ⚠️ シリアル通信（`src/mmu.rs`）

- blargg テスト ROM の文字出力（0xFF02 bit7 → 0xFF01 を stdout へ）のみ対応
- 転送タイミング・Serial 割り込み・実際のシリアルプロトコルは未実装

---

## 残実装タスク（優先度順）

### 1位: MBC3 RTC

**理由:** ポケモン金銀などの人気タイトルでゲーム内時計が動く。

**実装概要:**
1. `seconds / minutes / hours / day-lo / day-hi` の 5 レジスタ管理
2. リアル経過時間を RTC レジスタに反映
3. ラッチ書き込み（0x00 → 0x01）で現在値をスナップショット

### 2位: APU 精度向上

現在の実装で大半のゲームは音が出るが、以下の点で実機との差がある可能性：

- CH3 の周波数タイマー（T-cycle 単位の精度）
- Wave RAM アクセス競合（CH3 有効時の読み書き挙動）
- blargg `dmg_sound` テストによる検証
