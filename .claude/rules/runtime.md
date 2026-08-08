---
paths:
  - "src/pager/**"
  - "src/cli.rs"
  - "src/config.rs"
  - "src/main.rs"
  - "src/input.rs"
  - "tests/cli.rs"
---

# CLI と pager の決定事項

## 終了コードと不正な組み合わせ

`0` 成功 / `1` 入力・デコード・端末・出力の致命的エラー / `2` usage error。

- `--plain` と **明示指定された** `--color always` は usage error。判定には clap の `ValueSource` を使い、既定値との衝突は起こさない。`--plain --color never` は矛盾しないので受理する。
- `--watch` と stdin / `--no-pager` の併用は usage error。
- diagram の描画失敗は usage error でも fatal でもない。

## Config file のスコープ

config は **既存フラグの既定値を上書きするだけ**。テーマ色や keymap まで開かない。設定を増やすと意味論が増え、CLI フラグの契約と二重管理になる。

- 値の解析は clap の `ValueEnum::from_str` を使う。serde derive で書き直すと、受理される綴りが `--help` と二重管理になる。
- `serde(deny_unknown_fields)` を付ける。typo が黙って無効になる方が実害が大きい。未知のキー・不正な値は usage error。
- 探索順は `MDVU_CONFIG` → `$XDG_CONFIG_HOME/mdvu/config.toml` → `~/.config/mdvu/config.toml`。macOS でも `~/.config` に統一し、`dirs` 系の依存を足さない。
- `MDVU_CONFIG` が空文字なら読み込まない。テストはこれを使って実行環境のホームから独立する。
- 適用できない状況の設定値は黙って無視する（`watch = true` + stdin など）。フラグでの明示指定と違い、設定を書いた人が `mdvu -` を使うたびに usage error になるのは筋が悪い。

## 端末状態

`TerminalGuard` が raw mode / alternate screen / カーソル表示のすべての変更を所有し、`Drop` で戻す。正常終了・エラー・panic のいずれでも端末が使える状態で終わること。端末モードを他の場所で直接変更しない。マウスキャプチャは有効にしない。

## レイアウトの再実行

ドキュメントは起動時に一度だけ parse し、layout はリサイズと reload のときだけ再実行する。frame ごとに layout や diagram 描画を走らせない。reload は起動時とまったく同じ経路（`input::load` → `flavor::parse` → `diagram::resolve` → `layout_document`）を通るので、再読込した文書は開き直した文書と区別できない。

## File watch

`notify` の `RecommendedWatcher` を `std::sync::mpsc` で受け、既存の `crossterm::event::poll`（250ms）ループのタイムアウト側で回収する。async runtime は追加しない。

- **監視対象はファイルではなく親ディレクトリ。** エディタや Coding Agent の atomic save（一時ファイル → rename）は inode を差し替えるため、ファイル自体への watch は静かに外れる。イベントは file name で絞り込む。
- イベント種別は `EventKind::Access` だけを捨て、他はすべて変更として扱う。backend ごとに分類の粒度が違い、取りこぼしより余分な再読込の方が安い。
- デバウンスは 100ms。`notify-debouncer-*` は追加せず、`Instant` を引数に取る純粋な `Debounce` として持ち、タイマ非依存にテストする。
- 読み込み失敗（一時的な truncate、消失、非 UTF-8 への差し替え）は fatal にせず、status bar に出して直前の描画を保持する。

### watch テストが sandbox で落ちる

`a_write_to_the_file_is_noticed` と `an_atomic_save_is_noticed` は実際にファイルを書き換えて FSEvents を待つため、sandbox 内ではイベントが 1 件も届かず失敗する。**sandbox の制約であってコードの不具合ではない**（`PollWatcher` に差し替えると同じ 2 件が通る）。この 2 件を無効化したり `PollWatcher` へ切り替えたりせず、CI で検証する。

FSEvents は watch 開始直前の書き込みを起動後に報告することがあるため、テストは `Watch::new` 直後の積み残しを `settle` で捨ててから本題の操作を行う。

## OSC 8

ハイパーリンクは stdout backend（`output/ansi.rs`）だけが出力する。`ratatui` 0.30 の `Cell` はハイパーリンク属性を持たず、symbol にエスケープ列を埋めると差分描画で閉じ側が出力されず画面全体がリンク扱いになる。pager 内では下線付きテキストに留める。

## テスト

pager の対話部分は自動テストできない。状態遷移は `state.rs` / `event.rs` の純粋な unit test で固め、描画は `ratatui` の `TestBackend` で 1 フレーム描いて検証する。ロジックを `app.rs` のイベントループへ書き足すのではなく、`state.rs` の純粋関数側へ寄せる。Windows 端末上での pager 対話は未検証（CI は build と unit test のみ）。

`tests/cli.rs` と `tests/render.rs` のヘルパは `MDVU_CONFIG=""`、`COLORFGBG` / `NO_COLOR` 除去で開発者の環境設定を遮断している。新しいテストも同じヘルパ経由で起動する。
