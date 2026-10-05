# Windows / WSLでの4段階確認手順

この4段階は失われたクラウドコミットの復元ではなく、Windows上で再実装・再検証した新しい履歴です。実GPIO backendはありません。旧real-cartは書込みとpush-pull resetを行うため、新P9/P10 /OE配線で実行しないでください。

## チェックアウト

WSL Ubuntu内で実行します。`git status --short`に変更が出たら、この手順を止めて保存先を決めてください。reset --hardやcleanは使いません。

```sh
cd ~/workspace/gb_emu
git status --short
git fetch origin
git switch codex/outer-pins-lcd-20261004
git pull --ff-only
```

| 段階 | 新コミット | 内容 | 検証 |
|---|---|---|---|
| 基点 | 2ec104e | LCD変更と旧計画 | LCD配線変更済み、実機未検証 |
| 1 | 6030db7 | GPIO非依存RomReader、故障固定 | 全操作前後失敗・遮断失敗 |
| 2 | 2bc18bf | /RESET・bus-hold・電源の安全訂正 | 安全文書とcoreテスト |
| 3 | 9a13ab6 | 0x0134–014Dの26バイト一回取得 | 全26箇所の失敗停止、checksum/type/size |
| 4 | 下記でこのガイドを追加したコミットのSHAを取得 | 合成host CLIとCLIテスト | 正常・拒否・故障の終了コード |

段階4のSHAは自己参照を避け、ガイド追加コミットから取得します（失われた旧SHAは使用しません）。

```sh
stage4=$(git log -1 --format=%H --diff-filter=A origin/codex/outer-pins-lcd-20261004 -- docs/teensy_cart_test_stages.md)
printf '%s\n' "$stage4"
```

クリーンな作業ツリーで段階ごとに確認できます。detached HEADで編集せず、テスト・比較だけ行います。

```sh
git switch --detach 6030db7
cargo test -p gb-core --locked --offline
git switch --detach 2bc18bf
cargo test -p gb-core --locked --offline
git switch --detach 9a13ab6
cargo test -p gb-core --locked --offline
git switch --detach "$stage4"
cargo test -p gb-core --features host-poc --locked --offline
git diff 2ec104e.."$stage4" -- core/src/platform core/src/bin core/tests docs
git switch codex/outer-pins-lcd-20261004
```

過去版を確認する安全な戻し方は `git switch --detach <SHA>`、最新へ戻るのは最後のswitchです。detached状態で変更した場合も破棄せず、保存用ブランチ作成等を先に行います。共有ブランチの履歴を書き換えません。

## host専用PoC

リポジトリルートで実行。ROMファイル・シリアル・GPIOにはアクセスしません。合成期待値03/04/02は実カート測定値ではありません。時間の1単位も実時間ではありません。

```sh
cargo run -p gb-core --features host-poc --bin cart_header_poc --locked --offline -- --synthetic normal
cargo run -p gb-core --features host-poc --bin cart_header_poc --locked --offline -- --synthetic read-failure
cargo run -p gb-core --features host-poc --bin cart_header_poc --locked --offline -- --synthetic gate-refusal
```

normalは終了0、checksum/type/rom-size/ram-sizeは5、read-failureは4、gate-refusalは3。`--hardware`や不明な引数は2で拒否します。read-failureは0x0140で停止。実カートへ進める機能はありません。

## Teensyのビルドのみ

既存ツールを使用し、依存を更新しません。MakefileはROM絶対パスとtargetを設定します。

```sh
cd teensy
make build CARGO='cargo --locked --offline'
make FEATURES=real-cart build CARGO='cargo --locked --offline'
# 別ROMを使う場合
make ROM=/absolute/path/to/game.gb build CARGO='cargo --locked --offline'
```

通常版は既定で `../roms/game.gb` を使用し、real-cartではROMを無視します。上記はbuildのみ。flash/deployは実行しません。real-cartビルド成功は新配線での使用許可ではありません。

## LCD配線の変更

電源を切ってからDCをP9→P11、LCDのSDI/MOSIをP11→P12へ移します。SCK=P13、CS=GND、RST/BL=3.3V、GND共通。LCDのSDO/MISOは未接続とし、P12へ接続しません。音声・ボタンは変更しません。P9/P10は新/OE用の予約であり、まだ実GPIO制御されません。SD42–47も未使用です。

今回のソフト段階確認ではカートを接続しません。実機LCD表示・波形は未確認です。カート/RESETを245へつなぐ旧配線は使わず、[安全条件](teensy_cart_safety.md)のopen-drain・電源・bus-hold課題を先に解決します。

## 検証範囲

各段階でcoreテストを実施。最終段階では追加Rustファイルのrustfmt check、core全target check/test、通常/real-cart releaseビルドを実施します。結果はコミット完了時の作業報告を参照してください。ソフトテストは電気的安全・タイミング・実カート互換性の保証ではありません。
