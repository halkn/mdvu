# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`mdvu` は GFM と Azure DevOps Wiki Markdown を端末で描画する単一ファイル向けビューア（Rust 2024 edition、bin crate のみ、lib target 無し）。仕様・オプション・既知の制限は `README.md` にある。

## 検証と設計

検証基準は `.github/workflows/ci.yml` を正とする。`.claude/hooks/` は Claude Code の補助であり、他のエージェントでは自動実行を前提にしない。snapshot は既存の出力契約として扱い、仕様変更の理由がない差分は受け入れない。

文書の解析・リソース準備と、画面幅に依存する配置を分離する。リサイズで入力ファイルの再読込や構文解析を起こさない。フレーバー差は `flavor/` に閉じ、IR に端末や具体的なスタイルへの依存を持ち込まない。

各段の判断基準は `.claude/rules/rendering.md` と `.claude/rules/runtime.md` を参照する。画像関連の変更では `docs/mermaid-image.md` も読む。

## 常に守ること

- **ドキュメントの内容を実行しない。** JavaScript も iframe もネットワークもサブプロセスも起こさない。リンクを開くのは端末の仕事。
- **端末へブロッキングなクエリを投げない。** テーマ判定は `COLORFGBG` のヒントのみ。
- タスクの範囲外の整形・リファクタリングを混ぜない。

## 依存

- runtime 依存に Node.js / Chromium / WebView / 外部コマンドを持ち込まない。JavaScript engine を含む crate（`boa` / `v8` / `quickjs`）も入れない。
- 新しい外部 crate は 1 module に隔離する。`merman` は `diagram/mermaid.rs`、`resvg` は `diagram/raster.rs`、`syntect` は `layout/highlight.rs` が唯一の接触面。差し替えや不具合対応はその境界の内側で行う。
- feature は最小限に固定してある。`merman` は `ascii` と `render`（SVG まで）のみで、`raster` は持ち込まない（SVG 中の `<image>` が指すローカルファイルを読む resolver を使うため。理由は `docs/mermaid-image.md`）。`resvg` は `text` / `system-fonts` / `memmap-fonts` のみで、`raster-images` は入れない（図中の画像は描かない）。`syntect` は `parsing` / `default-syntaxes` / `regex-fancy` のみ（C 依存の `onig` を避け、色は `layout/theme.rs` が決めるので `default-themes` は不要）、`ratatui` は `crossterm` backend のみ。
- `cargo tree -d` が報告する 60 件前後の重複はすべて推移的依存のメジャーバージョン差で、直接依存の選択では解消できない。そのままにする。
- `RUSTFLAGS: -D warnings` を CI 全体に置かない。依存 crate のコンパイルにも適用され、約 390 crate のいずれかが警告を出すと落ちる。lint gate は `cargo clippy -- -D warnings` に任せる。
