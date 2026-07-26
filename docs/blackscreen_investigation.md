# Teensy ディスプレイ・ブラックアウト調査記録

最終更新: 2026-07-26

SAI1 オーディオ有効時に ST7789 がランダムなタイミングで真っ黒 (DISPOFF) になる不具合の
調査記録。**根本解決には未到達**で、試行一式は実験ブランチ `experiment/blackscreen-sai-dma`
(26f9665) に退避し、`master` はこの変更を取り込まず 0 ベースで再調査する方針。

---

## 1. 症状

- SAI1 オーディオを有効化すると、起動直後〜数十秒のランダムなタイミングで画面が真っ黒になる
- 黒化後も**音は継続**し、パニック・クラッシュはない。`gb.step()` / `draw()` は毎フレーム正常に呼ばれ、
  USB シリアルログ上も FPS 正常
- **DISPON (0x29) を 1 バイト送ると復帰する** → パネルは DISPOFF 状態に落ちている
- アンプ・スピーカーの接続有無、カートリッジ回路の有無は無関係

---

## 2. 確定した事実 (切り分け結果)

実験ブランチで 1 要素ずつ切り分けて判明したこと:

1. **引き金は SAI の FIFO 補充トラフィック (バス競合)**
   - LCD の DMA 走行中に SAI1 をマスク (= SAI FIFO 補充を停止) すると黒画面が消える
   - SAI1 割り込みを廃止し eDMA circular 駆動に変えても (補充トラフィックは継続) 黒画面は継続
   - → 引き金は「割り込みという CPU イベント」ではなく「SAI が FIFO 補充のため周辺バスへ出す
     トラフィックが LCD の DMA 転送と競合すること」

2. **LPSPI エラーは出ていない (アンダーラン/オーバーランではない)**
   - 黒化時、LPSPI SR の TEF(bit11)/REF(bit12) = 0 を実機ログで確認
   - NXP の CONT モード既知不具合 1 (TX アンダーラン)・2 (RX オーバーラン) は直接の原因ではない

3. **SPI エラーを伴わない「DC ずれ」で 0x28 がコマンド化**
   - DC (pin9) は独立 GPIO。0x28 がコマンドになるには送出の瞬間 DC=low である必要がある
   - GPIO 書き込みはアトミック (DR_SET/DR_CLEAR) なので割り込み競合ではない
   - `set_window()` の PIO 書き込みは embedded-hal の flush (BUSY+TXFIFO 両待ち) で送信完了を待つ。
     `finalize` の送信完了待ちも同条件に強化済みだが効果なし
   - → 通常のコードパスでは起こり得ないのに発生する = eDMA アービトレーションが LCD の
     continuous 転送のタイミング/フレーム同期を乱している下層の問題

4. **eDMA 優先度で LCD(ch0) を SAI(ch1) より高優先度に反転すると頻度が下がる**
   - デフォルトの固定優先度はチャネル番号順で SAI(ch1) > LCD(ch0) になっており、
     SAI 要求が LCD 転送を横取りしていた → バス競合が原因であることの裏付け

---

## 3. 真因の到達点

LCD の LPSPI4 continuous DMA 転送中に SAI の TX FIFO 補充 DMA トラフィックが競合し、LCD 側の
continuous 転送のタイミング/フレーム同期が乱れ、**DC=low の瞬間にピクセルバイト (0x28) が紛れて
DISPOFF コマンドと解釈される** (推定)。

正確な経路の最終特定には **ロジックアナライザで DC/SCK/CS を同時キャプチャ**し、黒化の瞬間に
0x28 が流れるとき実際に DC=low になっているかを直接観測する必要がある。

---

## 4. 試した対策と結果

| 対策 | 結果 |
|---|---|
| `start_dma`/`finalize` の生レジスタ操作を `interrupt::free` で囲む | 効果なし (その区間の競合ではない) |
| DMA 完了待ちの区間だけ SAI1 マスク | 効果なし (DMA 走行の大半はマスク外だった) |
| DMA 走行中ずっと SAI1 マスク | 黒画面消失。ただし音が死ぬので恒久策にできない |
| CONT=0 (continuous 廃止, CS=GND 固定) | 描画が壊れる (LPSPI 生制御が繊細) → 撤退 |
| TXWATER=15 (FIFO 先読み補充) | 白画面ハング → 撤退 |
| `finalize` 送信完了待ちを BUSY(MBF)+TXFIFO(FSR) 両方に強化 | 効果なし (送信完了待ちは真因でない) |
| SAI TX を eDMA circular 駆動化 (SAI1 割り込み廃止) | 音は出るが黒画面継続 (トラフィックは残る) |
| eDMA 優先度反転 (LCD > SAI) | 黒画面の頻度は低下、but 残存 |
| 毎フレーム DISPON 保険 | 頻度低下下でもちらつきとして体感に残る |

---

## 5. 現状

- 上記の対策一式は `experiment/blackscreen-sai-dma` (26f9665) に退避
- ちらつきが残り根本解決に未到達のため、`master` には取り込まず 0 ベースで再調査する

---

## 6. 技術メモ (再調査時の参照用レジスタ定数)

- **SAI1**: base=0x4038_4000, TCSR=+0x08 (FRDE=bit0, TE=bit31, W1C=FEF/SEF/WSF=bit18/19/20),
  TCR1=+0x0C (TFW ウォーターマーク=bit[4:0]), TDR0=+0x20
- **DMA リクエスト番号**: SAI1 TX=20 (IMXRT1060RM, 要実機検証), LPSPI4 TX=80
  (imxrt-hal `LPSPI_DMA_TX_MAPPING=[14,78,16,80]` で確認済み)
- **eDMA**: base=0x400E_8000, TCD 領域=+0x1000 (ch N の SADDR=0x400E_9000 + N*0x20),
  DCHPRI (ch0=base+0x103, ch1=base+0x102, bits[3:0]=CHPRI, bit6=DPA, bit7=ECP)
- imxrt-hal は SAI DMA 非対応 (Destination trait 未実装、FRDE を有効化する API なし) のため生レジスタ実装
- imxrt-dma の `set_source_circular_buffer` は要素数が 2 の累乗 + サイズ境界アラインが必須
- LCD は dma[0]、SAI は dma[1]。CS=pin10 制御・CONT=1 が動作実績あり (CS=GND 固定にすると
  CONT 転送が壊れる)

---

## 7. 次の一手候補

- **ロジックアナライザで DC/SCK/CS 同時キャプチャ** — 機序の最終特定 (最優先)
- LCD DMA のアンダーラン耐性 / eDMA プリエンプション設定のさらなる追い込み
- LCD 転送方式そのものの見直し (continuous mode 依存を減らす)
