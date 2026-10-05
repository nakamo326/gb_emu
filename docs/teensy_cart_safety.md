# 実カート接続の安全条件（未確定・実機接続前）

この文書は旧配線表より優先する。実GPIO統合・flash・実機試験はまだ行わない。

## /RESETを245のpush-pullから外す

カート側/RESETは監視IC等のopen-collector出力でLOW駆動され得る。245でHIGHを押し出す配線は競合し得るため、新設計ではU2制御バンクから外す。P37を用いた別open-drain回路、電圧変換、pull-up、電源断時の逆流対策は未確定。Teensy端子へ5Vを直接接続しない。U2の固定方向バンクは/RD・/WR・/CSの3信号、残り5chとなる。旧GpioCartは/RESETをpush-pull駆動する旧実装のため、新設計では使用禁止。

LVCHはB側も/OE無効時にbus-holdが働く。/OE=Hは信号の安全HIGHを保証しない。弱いpullで/WR等を必ずHIGHにできると仮定せず、hold電流、電源順序、監視回路を含め設計する。旧LVC用47kΩ信号pullは流用しない。

## 手持ちカートと期待値

ユーザーのカートはDMG-AKBJ-JPN（星のカービィ2）。[同型の公開記録](https://gbhwdb.gekkio.fi/cartridges/DMG-AKBJ-0/ben-black-1.html)にはDMG-DECN-02、MBC1B1、MM1026A、CR1616がある。別途DECN-10の記録もあるが、手持ちの実PCBは未確認。同型写真を現物の証明と扱わない。開封・追加写真は不要、実機試験は後日。

期待値はROM 512KiB、SRAM 8KiB、ヘッダ0x147/148/149 = 03/04/02。読み取って検証するための仮説であり、値が違えば停止して調査する。MBC書込み、RAM enable、リセット、エミュレーションには進めない。

## 電源

想定はTeensy USBのVIN/VUSB 5Vからcart VCCとシフタVB、Teensy 3.3VからVA、GND共通。[PJRC電源仕様](https://www.pjrc.com/store/teensy41.html#power)を確認する。VIN/VUSBの接続状態・外部電源との競合を確認し、On/Offで3.3Vを止めてもUSB側5Vが残る条件を扱う。MCU停止をカートの電源断とみなさない。片電源・逆流・信号注入・電池バックアップSRAM保護を回路で解決する。

## 採用方針と検討案

採用方針はP9=データ/OE、P10=残り3バンク共通/OE、最終データDIR=/RDを維持する。ただし/OEだけで起動安全性は保証できない。
初回試験の物理/WR inhibit、データ固定B→A、電源監視による隔離は検討案であり、採用済み回路ではない。固定方向案は最終DIR=/RDと区別する。抵抗値や待ち時間をソフトのテスト結果から確定しない。

## ソフトウェアの境界

RomReaderはGPIO非依存。ready gateは呼出側の申告で、安全回路の検証機能ではない。待ち時間の単位はbackend依存で実時間未保証。実機backendは提供しない。
エラー時はデータ/OE無効とdisable待ちを両方試み、DIRを変更せずfault固定する。遮断自体が失敗した場合もエラーに残すため、物理安全を保証したとは報告しない。ソフト再生成による復旧も実機で認める設計ではない。
