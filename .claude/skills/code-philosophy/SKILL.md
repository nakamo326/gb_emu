---
name: code-philosophy
description: Implementation and comment philosophy to apply when planning an implementation, designing an approach, fleshing out implementation details, writing/refactoring/reviewing code, and deciding what to comment. Applies from the design stage onward, not just when code is being written — complexity is introduced earliest at planning, so weigh these principles there too. Implementation — add no complexity beyond the problem's inherent complexity; aim for low cognitive load; when behavior is equivalent, choose the form that costs the reader fewer thinking steps. Comments — by default document what you deliberately chose NOT to do (rejected alternatives, avoided generalizations, known trade-offs), since the code already shows what was done; exceptionally, explain inherently hard logic or a variable whose role its name cannot convey. Reference whenever planning/designing an implementation, writing/simplifying code, or adding/removing comments. Triggers on "実装", "実装計画", "設計", "方針", "アーキテクチャ", "コメント", "リファクタ", "シンプルに", "命名", "plan", "design", "simplify", "comment".
---

# 実装とコメントの哲学（参照型）

これは実装を計画する/設計する/詳細を詰めるとき、コードを書く/リファクタする/レビューするとき、コメントを足す/削るときに参照する**判断基準**。手順を強制するものではなく、原則として使う。複雑性は設計・計画段階で最初に混入するので、コードを書く前から効かせる。迷ったら原則に照らして提案し、外れる場合は理由を言う。関連: [[commit-philosophy]]。

> **核心**：やったこと（実装）はコメントなしでも解るように書く。あえてやらなかったことはコードに痕跡が残らず、コメントでしか可視化できない。だからコメントの一等地は後者に充てる。

## 実装：本来的な複雑性だけを残す

1. **本来的な複雑性以上の複雑性を添加しない** — 問題そのものが持つ複雑さは引き受けるが、それ以上は持ち込まない。早すぎる抽象化・使われない一般化・将来の予測に基づく拡張点は、本来的でない複雑性。
2. **認知負荷の小さいコードを目指す** — 認知的複雑度（分岐・ネスト・状態・間接参照の多さ）を下げる。読み手が一度に頭に乗せる量を減らす。
3. **振る舞いが等価なら、読み手の思考ステップが一つでも少ない形を選ぶ** — 同じ挙動なら、追うのに必要な思考が少ないほうが良いコード。

## コメント：「あえてやらなかったこと」を書く

4. **デフォルトは「やらなかったこと」** — 採らなかった選択肢、避けた最適化、踏まなかった一般化、許容した制約・トレードオフ、ハマりどころの回避。これらは実装からは読み取れない。
5. **例外1：本質的に複雑・難解なロジック** — どうしても本来的な複雑性が高い箇所は、「何をしているか」の説明を書いてよい。
6. **例外2：名前で役割を明示できない変数等** — わかりやすい命名が困難で、名前から役割が立ち上がらないものは説明を添えてよい。ただしまず命名で解決できないか試す。

## なぜ

やったことをコメントで繰り返すのは重複であり、コードとコメントが乖離して**コメントが嘘になる**リスクを生む。実装が自明に語れているなら、その説明コメントは不要。一方「なぜそうしなかったか」はコードに残らないので、書かなければ永久に失われる。
