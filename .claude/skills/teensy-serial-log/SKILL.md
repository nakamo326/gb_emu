---
name: teensy-serial-log
description: Teensy 実機からの USB シリアルログ (imxrt-log) を WSL2 経由で取得・モニターするスキル。COM ポートの検出、PowerShell 経由の読み出し、フィルタ付きモニターの起動を行う。Triggers on "シリアルログ", "serial log", "ログ取得", "COM", "モニター", "teensy log".
---

# Teensy USB シリアルログ取得

WSL2 環境では USB デバイスが直接見えないため、Windows 側の PowerShell 経由で COM ポートを読む。

## 手順

### 1. COM ポート検出

```bash
powershell.exe -Command "[System.IO.Ports.SerialPort]::GetPortNames()"
```

Teensy CDC は COM1 以外のポート (COM3 等) として現れる。デバイスが見つからない場合はユーザーに Teensy の接続・書き込み完了を確認する。

### 2. モニター起動

Monitor ツールを使い、PowerShell でシリアルポートを読む。**出力過多による自動停止を防ぐため、必ずフィルタを掛ける。**

#### フィルタの選択

ユーザーの目的に応じて以下から選択する：

- **異常検知**: `drops:` が 0 より大きい、または `peak:` が 100% を超えるログのみ通知。安定動作の確認に使う。
- **全ログ**: すべての行を出すが、**Monitor ではなく Bash (run_in_background)** を使い、完了後にまとめて読む。リアルタイム性は不要だが全データが欲しい場合。

#### 異常検知モニター (推奨)

```
Monitor({
  description: "Teensy serial - anomalies only",
  persistent: true,
  command: `powershell.exe -ExecutionPolicy Bypass -Command "
    $port = New-Object System.IO.Ports.SerialPort 'COM<N>', 115200
    $port.ReadTimeout = 2000
    $port.Open()
    while ($port.IsOpen) {
      try {
        $line = $port.ReadLine()
        if ($line -match 'peak:(\\d+)%.*drops:(\\d+)') {
          $peak = [int]$Matches[1]
          $drops = [int]$Matches[2]
          if ($peak -gt 100 -or $drops -gt 0) {
            Write-Host $line
          }
        }
      } catch [System.TimeoutException] {}
    }
  " 2>&1`
})
```

#### 全ログ収集 (バックグラウンド Bash)

Monitor の出力制限を回避するため、ファイルに書き出して後で読む：

```bash
Bash(run_in_background: true, timeout: 60000, command:
  powershell.exe -ExecutionPolicy Bypass -Command "
    $port = New-Object System.IO.Ports.SerialPort 'COM<N>', 115200
    $port.ReadTimeout = 2000
    $port.Open()
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($port.IsOpen -and $sw.Elapsed.TotalSeconds -lt 55) {
      try {
        $line = $port.ReadLine()
        Write-Host $line
      } catch [System.TimeoutException] {}
    }
    $port.Close()
  " 2>&1 | tee <scratchpad>/serial_log.txt
)
```

完了後 `Read(<scratchpad>/serial_log.txt)` で全ログを確認できる。

### 3. 注意事項

- **Monitor の出力制限**: Monitor は短時間に多くのイベントを出すと自動停止する。毎秒出力があるログ（overlay 時間等）を全件通知するとすぐ停止するため、Monitor を使う場合は必ずフィルタで絞ること。
- **COM ポート番号**: Teensy をリセット/再書き込みするとポート番号が変わることがある。接続エラーが出たら再検出する。
- **ボーレート**: `imxrt-log` の USB CDC はボーレート設定に依存しないが、PowerShell の SerialPort は値を要求するため 115200 を指定する。
- **停止**: TaskStop でモニターを停止する。COM ポートはモニター停止時に自動解放される。
