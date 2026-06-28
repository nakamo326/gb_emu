# ADR-001: ROM バンクの RAM キャッシュ化

## ステータス

提案

## 背景

Teensy 4.1 向けビルドでは ROM を `include_bytes!` で Flash に埋め込み、XIP (Execute In Place) で直接読み出している。D-cache が有効なため通常のフレーム処理は予算の約 31% で収まるが、ゲーム内のシーン遷移（マップ切替・建物の出入り等）で MBC バンク切替が発生すると、未キャッシュの Flash 領域へのアクセスが集中し **単一フレームが予算の 300-500% に跳ねる** 現象を確認した。

### 計測データ (2026-06-29)

| 状態 | FPS | peak | avg | drops |
|------|-----|------|-----|-------|
| 安定時 | 59.7 | 35-53% | 31% | 0 |
| スパイク時 | 56-58 | 272-459% | 34-42% | 1 |

オーバーレイ描画（~2.1ms）は毎回一定でスパイクと無関係であることを個別計測で確認済み。原因は **Flash (FlexSPI) のキャッシュミス**。

### なぜ Flash が遅いか

- Cortex-M7 の D-cache は 32KB。GB ROM バンクは 16KB 単位で最大 128 バンク (2MB)
- バンク切替時、新バンクのデータは D-cache に載っていない
- FlexSPI 経由の Flash 読み出しは 1 アクセスあたり ~200-500ns（キャッシュヒット時の ~3ns と比較して 100 倍以上遅い）
- 1 フレーム中に新バンクから大量のタイルデータを読み込むと、キャッシュミスが累積して 1 フレームだけ極端に遅くなる

## 決定

**MBC バンクレジスタ書き込み時に、アクティブバンクの 16KB を Flash から DTCM の RAM バッファにコピーする。以降の ROM 読み出しは RAM バッファから行う。**

## 実装方針

### 対象ファイル

`teensy/src/sdcard.rs` — `FlashCart` 構造体

### データ構造の変更

```rust
// ROM バンクの RAM キャッシュ (DTCM に配置)
static mut BANK0_CACHE: [u8; 0x4000] = [0; 0x4000]; // 0x0000-0x3FFF 用 (16KB)
static mut BANKN_CACHE: [u8; 0x4000] = [0; 0x4000]; // 0x4000-0x7FFF 用 (16KB)

struct FlashCart {
    rom: &'static [u8],
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    mode: bool,
    // 追加: キャッシュされているバンク番号 (初期値は無効値)
    cached_bank0: u8,  // BANK0_CACHE に載っているバンク
    cached_bankn: u8,  // BANKN_CACHE に載っているバンク
}
```

### 読み出しパスの変更

```rust
// Before: Flash 直読み
self.rom[offset]

// After: RAM キャッシュから読み出し
unsafe { BANKN_CACHE[addr as usize - 0x4000] }
```

### バンク切替時のコピー

```rust
// write() の 0x2000-0x3FFF (rom_bank 変更) で:
fn sync_bankn_cache(&mut self) {
    let bank = self.effective_rom_bank();
    if bank != self.cached_bankn {
        let offset = bank as usize * 0x4000;
        let end = (offset + 0x4000).min(self.rom.len());
        unsafe {
            BANKN_CACHE[..end - offset].copy_from_slice(&self.rom[offset..end]);
            if end - offset < 0x4000 {
                BANKN_CACHE[end - offset..].fill(0xFF);
            }
        }
        self.cached_bankn = bank;
    }
}
```

bank0 (0x0000-0x3FFF) も MBC1 mode=1 かつ大容量 ROM の場合にバンク切替が発生するため、同様にキャッシュする。

### 同期タイミング

以下のレジスタ書き込みでキャッシュを更新する：

| アドレス | レジスタ | 影響するキャッシュ |
|----------|----------|-------------------|
| 0x2000-0x3FFF | rom_bank | BANKN_CACHE |
| 0x4000-0x5FFF | ram_bank | BANKN_CACHE (大容量 ROM 時)、BANK0_CACHE (mode=1 時) |
| 0x6000-0x7FFF | mode | BANK0_CACHE (mode=1 かつ大容量 ROM 時) |

### 初期化

`FlashCart::new()` の後、最初の `read()` の前にバンク 0 とバンク 1 のキャッシュを充填する。`new()` は `const fn` なのでコピーはできないため、別途 `init_cache(&mut self)` メソッドを用意し、`main()` から呼ぶ。

## メモリコスト

| 領域 | サイズ | 配置 |
|------|--------|------|
| BANK0_CACHE | 16KB | DTCM (.bss) |
| BANKN_CACHE | 16KB | DTCM (.bss) |
| **合計** | **32KB** | |

現在の DTCM 使用量: スタック 176KB + BSS/data ~135KB = ~311KB / 320KB。残り ~9KB では 32KB は入らない。

### 対策: スタックサイズの調整

GameBoy 構造体のスタック使用量は ~86KB。現在のスタック 176KB は過大。**128KB に縮小** すれば 48KB を解放でき、32KB のキャッシュバッファを確保できる (残り ~16KB の余裕)。

## パフォーマンス見込み

- **バンク切替コスト**: 16KB の `copy_from_slice` ≈ DTCM←Flash で ~50-100μs（予算 16,742μs の 0.3-0.6%）
- **通常フレーム**: ROM 読み出しが DTCM 速度 (1-2 サイクル) になり、avg がわずかに改善
- **シーン遷移**: キャッシュミスによるスパイクが解消。バンク切替コスト (~100μs) のみ

## 採用しなかった代替案

- **FlexSPI プリフェッチ有効化**: 効果が限定的。シーケンシャルアクセスには効くが、タイルデータの読み出しはランダムアクセスに近い
- **D-cache 手動プリロード**: バンク切替タイミングで `__DSB` + ループ読み出しで D-cache にプリロード。32KB の D-cache にバンク全体 (16KB) が載る保証がなく、他のデータを追い出すリスクがある
- **OCRAM 配置**: ROM 全体を OCRAM に置く案。ROM が数 MB になりうるため OCRAM (512KB) に収まらない
