# Teensy 4.1 GPIO・周辺機能ピン割り当て

更新日: 2026-09-24

Teensy 側の **P0〜P41** を、現在の [`teensy/src/main.rs`](../teensy/src/main.rs)、[`teensy/src/cartridge.rs`](../teensy/src/cartridge.rs)、[`teensy/src/input.rs`](../teensy/src/input.rs)、[`teensy/src/audio.rs`](../teensy/src/audio.rs) に合わせて示す。表の `GPIOx[y]` は GPIO ポートとビット番号。LPSPI4 と SAI1 の行は GPIO モードではなく周辺機能に切り替える。

- **通常ビルド**: ROM を Flash に埋め込む。カートリッジ用ピンはファームウェアから設定しない。
- **`real-cart` ビルド**: `make FEATURES=real-cart build`。実 GB カートリッジ用のアドレス、データ、制御線を追加する。
表の「同左」は通常ビルドと同じ用途、`—` はそのビルドで用途を割り当てていないことを示す。`real-cart` ビルドでは P0〜P41 の全ピンをコード上で割り当てているが、画面の CS と MISO のように外部接続を省略できるピンもある。

| Teensy ピン | 通常ビルド | `real-cart` ビルド | GPIO / 周辺機能 |
|---:|---|---|---|
| P0 | — | カート A6（出力） | GPIO1[3] |
| P1 | — | カート A7（出力） | GPIO1[2] |
| P2 | — | カート A10（出力） | GPIO4[4] |
| P3 | — | カート A11（出力） | GPIO4[5] |
| P4 | — | カート A12（出力） | GPIO4[6] |
| P5 | — | カート A13（出力） | GPIO4[8] |
| P6 | — | カート A14（出力） | GPIO2[10] |
| P7 | 音声 TX_DATA（出力） | 同左 | SAI1 |
| P8 | — | カート A15（出力） | GPIO2[16] |
| P9 | 画面 DC（出力） | 同左 | GPIO2[11] |
| P10 | 画面 PCS0（出力） | 同左 | LPSPI4 |
| P11 | 画面 MOSI / SDO（出力） | 同左 | LPSPI4 |
| P12 | 画面 MISO / SDI（入力） | 同左 | LPSPI4 |
| P13 | 画面 SCK（出力） | 同左 | LPSPI4 |
| P14 | — | カート D0（双方向） | GPIO1[18] |
| P15 | — | カート D1（双方向） | GPIO1[19] |
| P16 | — | カート D5（双方向） | GPIO1[23] |
| P17 | — | カート D4（双方向） | GPIO1[22] |
| P18 | — | カート A1（出力） | GPIO1[17] |
| P19 | — | カート A0（出力） | GPIO1[16] |
| P20 | — | カート A8（出力） | GPIO1[26] |
| P21 | — | カート A9（出力） | GPIO1[27] |
| P22 | — | カート D6（双方向） | GPIO1[24] |
| P23 | 音声 MCLK（外部未接続） | カート D7（双方向） | 通常: SAI1 / `real-cart`: GPIO1[25] |
| P24 | — | カート A4（出力） | GPIO1[12] |
| P25 | — | カート A5（出力） | GPIO1[13] |
| P26 | 音声 TX_BCLK（出力） | 同左 | SAI1 |
| P27 | 音声 TX_SYNC / LRCLK（出力） | 同左 | SAI1 |
| P28 | ボタン SEL_DIR（出力） | 同左 | GPIO3[18] |
| P29 | ボタン SEL_ACT（出力） | 同左 | GPIO4[31] |
| P30 | ボタン IN0（入力） | 同左 | GPIO3[23] |
| P31 | ボタン IN1（入力） | 同左 | GPIO3[22] |
| P32 | ボタン IN2（入力） | 同左 | GPIO2[12] |
| P33 | — | カート /RD（出力） | GPIO4[7] |
| P34 | — | カート /WR（出力） | GPIO2[29] |
| P35 | — | カート /CS（出力） | GPIO2[28] |
| P36 | ボタン IN3（入力） | 同左 | GPIO2[18] |
| P37 | — | カート /RESET（出力） | GPIO2[19] |
| P38 | — | カート A2（出力） | GPIO1[28] |
| P39 | — | カート A3（出力） | GPIO1[29] |
| P40 | — | カート D2（双方向） | GPIO1[20] |
| P41 | — | カート D3（双方向） | GPIO1[21] |

## 配線と切り替えの注意

- 画面ドライバは現在 **ST7789**。P10 はコードで LPSPI4 の PCS0 に設定するが、現在の画面配線では CS を GND に固定しており、P10 へは接続しない。画面の MISO も接続省略可。DC は P9。画面の RST とバックライトは 3.3V に固定し、P8 を画面用に使わない。
- 音声は P7・P26・P27 を外部へ配線する。P23 は音声初期化 API に渡す MCLK だが外部には配線しない。`real-cart` では音声初期化後に P23 を GPIO に戻し、カートの D7 に使う。
- ボタンの P30・P31・P32・P36 は内部 22kΩ プルアップ付き入力。P28 を LOW にすると方向キー、P29 を LOW にすると A/B/Select/Start を読む。交点は[ボタン配線ガイド](teensy_button_wiring.md)を参照。
- `real-cart` ではカート線を **SN74LVC16T245 ×2** 経由で接続する。P33 の /RD は U2 のデータバンクの DIR にも直結する。4 本の /OE は GND 固定で、**P42 はファームウェアでもレベル変換回路でも未使用**。IC とカート端子までの対応は[実カートリッジ配線ガイド](real_cart_wiring.md)を参照。
- USB シリアルは USB_OTG1 を使用し、この表の汎用ピンには割り当てない。
