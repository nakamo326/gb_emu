# Teensy 4.1 外周ピン見直し計画

> 安全訂正: cart /RESETはカート側からLOW駆動され得るため、245のpush-pullから外す別open-drain案が必要です。旧/RESET配線は使用しないでください。B側bus-holdは/OE無効でも動作するため、弱pullで安全HIGHを保証できません。[安全条件](teensy_cart_safety.md)を優先してください。

更新日: 2026-10-05。基点: `aa3febe306f30020902c26a0b07527eba0dd0a3b`。

## 実装済みと未実装

LCD変更はコミット `f8f50ed` で実装済み。通常・real-cartのreleaseビルドは成功。実配線、表示、波形は未検証。
P9/P10は確保しただけで/OE制御は未実装。現行GpioCartは書込みを含む旧ドライバのままで、新回路用PoCではない。
今回の追加変更は文書のみ。追加/OE実装、ハード書込み、ソフトの新規インストールは行わない。

## ピンとモジュールの採用方針

外周P0–P41だけを使用し、SD端子P42–P47を避ける。全ピンは[更新ピン表](teensy_gpio_pin_assignments.md)を参照。

| 信号 | 採用ピン・配線 | 状態 |
|---|---|---|
| LCD DC | P11 GPIO2[2] | 実装済み |
| LCD DATA | P12 LPSPI4 SINを送信に変更 | 実装済み |
| LCD SCK | P13 LPSPI4 SCK | 維持 |
| LCD CS / RST / BL | GND / 3.3V / 3.3V | 固定配線 |
| データ8bit /OE | P9 GPIO2[11] → U2 OE1 | 予約、未実装 |
| アドレス・制御3バンク共通 /OE | P10 GPIO2[0] → U1 OE1・OE2、U2 OE2 | 予約、未実装 |
| データDIR | P33（3.3V側/RD）→ U2 DIR1 | 維持 |
| D0–D7 | P14,15,40,41,17,16,22,23 | 維持 |
| A0–A15 | P19,18,38,39,24,25,0,1,20,21,2,3,4,5,6,8 | 維持 |
| /RD、/WR、/CS、/RESET | P33、P34、P35、P37 | 維持 |
| 音声 DATA/BCLK/SYNC | P7/P26/P27 | 維持 |
| ボタン SEL_DIR/SEL_ACT、IN0–IN3 | P28/P29、P30/P31/P32/P36 | 維持 |

P23は音声初期化後にカートD7へ戻す既存処理を維持する。LCD側SDI/MOSIをP12、DCをP11へ移し、LCD側SDO/MISOは未接続。旧LCD配線をP9/P10から外す。CS固定なのでSPI共有は前提にしない。

AE-LLCNV-LVCH16T245を2個使用。両方A=3.3V、B=5V、GND共通。U1の2バンクがアドレス16bit、U2バンク1がデータ8bit、U2バンク2が/RD・/WR・/CSの3bit（/RESETは別回路未確定）。固定方向3バンクのDIRは3.3Vへ接続する。

以下は依頼で指定された**モジュールヘッダ番号**。接続前に現物シルクとモジュール回路図を照合する。IC端子番号とは異なる。

| 制御名 | ヘッダ番号 | IC本体TSSOP端子番号（比較用） | 接続 |
|---|---:|---:|---|
| OE1 | 47 | 48 | U1=P10、U2=P9 |
| OE2 | 46 | 25 | U1/U2ともP10 |
| DIR1 | 45 | 1 | U1=3.3V、U2=P33 |
| DIR2 | 44 | 24 | U1/U2とも3.3V |

工場実装のOE/DIR 10kΩプルアップを維持する前提。P10は10kΩが3並列で約3.3kΩ、LOW時の抵抗電流は約0.99mA。P9は約0.33mA、P33にはDIR用約0.33mAが加わる。容量・漏れ電流を含む駆動条件も確認する。

LVCHの信号端子にはbus-holdがあるため、旧LVC用47kΩプル一式を流用しない。未使用5chの処理もLVCHとモジュール資料に従い確定する。bus-holdは起動時の/RD・/WR・/CS・/RESETの所望値を保証しない。制御線の安全値はbus-holdとの競合、VIH/VIL、電源順序を含め別途設計する。

## LCD実装の根拠

`board::LpspiPins`はP10/P11もSPIへmuxするため使用しない。`Lpspi::<(),4>::without_pins`で生成し、`hal::iomuxc::lpspi::prepare`はP12/P13だけに適用。P11はGPIO DC。
コンストラクタ後にSPIを無効化し、`is_enabled()`がfalseになるまで待つ。CFGR1をRMWし、PINCFG[25:24]=3（SIN送信/SOUT受信）、OUTCFG[26]=0を設定、MASTER/SAMPLE等の他ビットを保持する。P11は受信SOUTとしてmuxしない。

元のBSP root clock設定と初期24MHz指定を維持。CCRも従来どおりSCKDIV=2、DBT/PCSSCK/SCKPCS=1（132MHz rootから33MHz）。HAL WriteのRXMSK=1と既存DMA TCRのRXMSK=1を確認済み。DMAチャネル、TX request=80、TDR=0x403A0064、DmaDisplayの汎用SPI型は変更していない。

## 次段階: /OE設計と読み取り専用PoC

1. 電源立上り・MCU reset中も両/OE=Hをハードで確保する。ファームウェア開始前はソフトだけでは保証できない。
2. P9/P10出力ラッチをHにしてからGPIO出力へ。データは入力、/RD=/WR=/CS=H、アドレス既知値、/RESETは回路で決めた安全状態へ設定する。
3. GPIO・方向・電源の安定後にP10=Lとし、アドレス/制御を有効化。/RESETパルスと回復待ちを行う。P9はHのまま。
4. 条件を満たした場合だけPoCへ。終了・異常時はデータ/OE=Hで停止する。

共通/OEがHの間はcart側制御線もHi-Z。/WR=/RD=/CSの非アサートと/RESETの安全状態をハードで保証する必要がある。USB再接続、Teensy停止、3.3V/5Vの投入順、片電源も検証対象。

| 読取り段階 | データ/OE | /RD (=DIR) | Teensyデータ | 待つ条件 |
|---|---|---|---|---|
| Idle・アドレス設定 | H | H | 入力 | cart出力解放、アドレス/CS安定 |
| 読み開始 | H | L | 入力 | DIR整定、cartアクセス時間 |
| 接続・サンプル | L | L | 入力 | 245 enable/伝搬時間 |
| 切断 | H | L | 入力 | 245 tDIS最大値以上 |
| 読み終了 | H | H | 入力 | /RD伝搬 + cart tHZ最大値以上 |
| 復帰 | H | H | 入力 | /CS解除、次アクセスへ |

方向切替の**前後を/OE=Hで覆う**。Teensyデータが入力でも、/RD=Hに戻すと245がA→Bへ変わりcartの出力解放と競合し得る。既存コードの「入力なので衝突しない」というコメントを新設計の根拠にしない。
tDIS、tEN、DIR setup、cart tACC/tHZ、制御伝搬を別制約として最大値で積み上げる。A3.3V/B5V、温度、負荷容量の該当表から確定し、バリア/周辺書込み完了と実波形を確認する。既存ACCESS_DELAY=180とasm::delayのコメントだけでns値や解放時間を保証しない。

最初のPoCはエミュレータへ接続せず、固定ROM領域のヘッダ反復読出し・チェックサムに限定。/WR常時H、データ常時入力とし、CartridgeBus::writeを呼ぶ構成を使わない。MBCバンク変更も書込みなので対象外。
起動安全回路・最悪時タイミング・電源条件が未確定のため、実GPIO PoC/OEドライバは有効化しない（GPIO非依存モデルのみ別途実装）。書込みは別コミットで、/OE=H → cart出力解放 → /RD=H・GPIO出力設定 → DIR/データ整定 → /OE=L → /WRパルス → データhold → /OE=H・tDIS待ち → GPIO入力復帰の順を検証してから実装する。

## 実機検証の順序と残条件

1. カート未接続でLCDのDC/DATA/SCK、33MHz波形、HALコマンド送信、DMA連続描画を確認。
2. 音声・ボタンを併用し、既知の画面暗転問題との差を確認。
3. 起動安全回路を確定し、カート未接続で電源投入/reset時の制御線・両/OE・DIRを測定。
4. 読取りPoCで両側データ・/RD・/OEを同時測定し、競合がないこととヘッダ反復一致を確認。
5. 上記を満たした後に書込み対応を別途実装・検証する。

## ビルド検証記録

2026-10-04にLCDコミットf8f50edの実装を検証（最後の変更はPINCFG説明コメントのみ）。既存WSL Ubuntu / stable Rust / thumbv7em-none-eabihf、teensy4-bsp 0.5.2 / imxrt-hal 0.5.14を使用。以下はteensy/で両方成功、各11件の未使用コード等の警告あり。

```sh
GB_ROM=/home/nakamo/workspace/gb_emu/roms/game.gb cargo build --release --locked --offline
cargo build --release --features real-cart --locked --offline
```

最初の通常ビルドはbuild.rsの既定ROM相対パスで失敗。既存Makefileと同じ絶対パス指定で解消し、build.rsは未変更。2026-10-05はコード変更なしのため再ビルドせず、この成功結果をf8f50edに紐付ける。前回/tmpログは再開時には残っていない。

ignoredの既存Cargo.lockは更新せず、開始時・ビルド後・今回再開時のSHA256一致を確認:

- teensy/Cargo.lock: `f279685839f89423e20cfd9573658a7d6153cc37e43206c303669afdd3cc6b46`
- Cargo.lock: `6f54f3bf4025525a6ea1f05af400a8b44a120195ecfd0dad02347a3999baa607`

## 参照資料

- [NXP i.MX RT1060 Reference Manual Rev.3（PJRC配布）](https://www.pjrc.com/teensy/IMXRT1060RM_rev3.pdf#page=2878): 前回PDF取得は失敗。CFGR1のビット定義はローカルimxrt-ral 0.5.4とも照合。
- [imxrt-hal LPSPIソース](https://github.com/imxrt-rs/imxrt-hal/blob/aeb829798d80bcdf0b03664de99fc080b8b851f4/src/common/lpspi.rs#L450-L560): 使用中0.5.14でもwithout_pins、初期化、Writeのreceive_data_mask=trueを確認。
- [TI SN74LVCH16T245 datasheet](https://www.ti.com/lit/ds/symlink/sn74lvch16t245.pdf): bus-hold、/OE、DIR、電源、タイミング条件。
- ヘッダ番号と工場抵抗は依頼指定に基づく。モジュール一次資料の取得は未完了なので、実接続前に現物・回路図で照合する。
