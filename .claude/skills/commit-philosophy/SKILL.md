---
name: commit-philosophy
description: Commit philosophy and principles to apply when creating git commits or structuring/rewriting commit history. Covers commit granularity (1 intent per commit; separating mechanical from semantic changes), ordering (behavior-preserving refactors first, thin isolated behavior-change commits, cleanup last), message format (Conventional Commits with the "why" in the body), what to exclude (temporary tuning, secrets), and staging/rewrite practices. Reference this whenever committing changes, splitting a working tree into commits, or planning a commit series. Triggers on "commit", "コミット", "コミットを分けて", "履歴を整える".
---

# コミット哲学（参照型）

これはコミットを作る/履歴を整えるときに参照する**判断基準**。手順を強制するものではなく、分割・順序・メッセージ・除外を判断するための原則として使う。迷ったら原則に照らして提案し、外れる場合は理由を言う。

> **目的**：レビュー容易性・`git bisect`/`cherry-pick` の効きやすさ・「なぜ」の追跡可能性を最大化する。

## 粒度

1. **1コミット = 1つの意図** — 論理的に独立した最小単位。単位は行数ではなく*意図*。
2. **機械的変更と意味的変更を混ぜない** — フォーマット・リネーム・移動・自動生成物・バイナリは、振る舞いを変える変更と別コミットにする。
3. **理由を共有する随伴変更は畳む** — ある変更によって初めて成立する除去・追従（例：設計変更で dead code 化するコードの削除）は、別コミットにせずその変更コミットに含める。「1意図」の*意図*は論理であって機械的な行数ではない。

## 順序

4. **挙動変更は隔離し、各コミットを最小化する**（「集約」ではない）
   - 振る舞いを変える差分は、挙動等価なリファクタと**混ぜない**（リファクタは先のコミットへ出す）。
   - 前準備を先に済ませることで、振る�いを変えるコミットは**最小の差分**になる。
   - **挙動変更が複数あれば複数コミットに分ける**（1つに押し込まない）。
   - 本質的に大きい機能変更は、各段階で*壊れない/フラグで gate される*単位に割り、**段階的な挙動変更コミット**にする。
5. **挙動不変を先、挙動変更を後、後片付けはさらに後** — 挙動等価なリファクタを前に積み、振る舞いを変える薄いコミットを置き、dead code 削除等の cleanup は最後。リスク・ロールバック・レビュー範囲を絞るため。

## メッセージ

6. **件名＝何を / 本文＝なぜ** — Conventional Commits（`feat`/`fix`/`docs`/`style`/`refactor`/`test`/`chore`/`perf`/`revert`）。件名は簡潔（体言止め）、本文に「なぜそうしたか」を残す（未来の自分・レビュアーへの贈り物）。自明なら件名のみ可。日本語で書く。

## コミットに含めない

7. **検証用の一時チューニング・デバッグ出力・実験コードは含めない**（検証後に revert）。**秘密情報（トークン等）は直書きせず env/flag 経由**。

## 運用（手段）

8. **staging は選別的に** — `git add .` を避け、ファイル/ハンク単位（`git add -p`）で「意味のあるまとまり」だけ載せる（原則1・2・7を実現する手段）。
9. **提出前に履歴を整える** — `--amend` / `rebase -i`(fixup) で作業ノイズを畳み、cherry-pick 可能なきれいな列に再構築してから出す。
10. **rewrite の境界** — 履歴の rewrite は「まだ共有していない自分の作業ブランチ」に限る。**他者が依存している/マージ済みの履歴は rewrite しない**。
