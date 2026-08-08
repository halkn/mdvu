# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`mdvu` は GFM と Azure DevOps Wiki Markdown を端末で描画する単一ファイル向けビューア（Rust 2024 edition、bin crate のみ、lib target 無し）。仕様・オプション・既知の制限は `README.md` にある。

## コマンド

```console
cargo test --all-features                     # 全テスト
cargo test --test render                      # golden rendering のみ
cargo test --test cli                         # CLI 統合テストのみ
cargo test --test render gfm_tables_80        # 単一テスト
cargo test --lib flavor::                     # モジュール名で絞った unit test
cargo insta review                            # snapshot 差分のレビュー
INSTA_UPDATE=always cargo test --test render  # snapshot 再生成
```

`cargo fmt` と `cargo clippy` は手で回さなくてよい。編集した `.rs` は PostToolUse hook が rustfmt にかけ、応答終了時に Stop hook が clippy と未レビュー snapshot を検査する（`.claude/hooks/`）。CI は `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` / `cargo test --all-features` を Linux・macOS・Windows で回す。

リリースは `/release`。

## アーキテクチャ

パイプラインは一方向で、各段が次の段の入力だけを作る:

```text
input(src/input.rs) → flavor::parse → diagram::resolve → layout::layout_document → output / pager
```

- **`markdown/`** — `pulldown-cmark` によるパースと renderer-neutral な Document IR。
- **`flavor/`** — `gfm.rs` と `azure_devops.rs`。フレーバー差はここに閉じ、layout 以降に持ち込まない。
- **`diagram/`** — Mermaid をレイアウト前に一度だけ描画し、`DiagramBlock::rendered` に格納する。
- **`layout/`** — IR を `RenderedLine` / `RenderedSpan` の面へ変換する。span は色ではなく意味的な `StyleRole` を持つ。
- **`output/`** — stdout backend（`ansi.rs` / `plain.rs`）。
- **`pager/`** — `app.rs` のイベントループ、`state.rs` の純粋な状態遷移、`view.rs` の ratatui 描画、`watch.rs` のファイル追従。
- **`cli.rs` / `config.rs`** — フラグ定義と `~/.config/mdvu/config.toml`。

各段の決定事項は `.claude/rules/` にあり、該当ファイルを読んだ時点で読み込まれる（`rendering.md` = markdown / flavor / diagram / layout、`runtime.md` = cli / config / pager）。

## 常に守ること

- **ドキュメントの内容を実行しない。** JavaScript も iframe もネットワークもサブプロセスも起こさない。リンクを開くのは端末の仕事。
- **端末へブロッキングなクエリを投げない。** テーマ判定は `COLORFGBG` のヒントのみ。
- タスクの範囲外の整形・リファクタリングを混ぜない。

## 依存

- runtime 依存に Node.js / Chromium / WebView / 外部コマンドを持ち込まない。JavaScript engine を含む crate（`boa` / `v8` / `quickjs`）も入れない。
- 新しい外部 crate は 1 module に隔離する。`merman` は `diagram/mermaid.rs`、`syntect` は `layout/highlight.rs` が唯一の接触面。差し替えや不具合対応はその境界の内側で行う。
- feature は最小限に固定してある。`merman` は `ascii` のみ（`render` / `raster` と image 系は持ち込まない）、`syntect` は `parsing` / `default-syntaxes` / `regex-fancy` のみ（C 依存の `onig` を避け、色は `layout/theme.rs` が決めるので `default-themes` は不要）、`ratatui` は `crossterm` backend のみ。
- `cargo tree -d` が報告する 30 件前後の重複はすべて推移的依存のメジャーバージョン差で、直接依存の選択では解消できない。そのままにする。
- `RUSTFLAGS: -D warnings` を CI 全体に置かない。依存 crate のコンパイルにも適用され、286 crate のいずれかが警告を出すと落ちる。lint gate は `cargo clippy -- -D warnings` に任せる。
