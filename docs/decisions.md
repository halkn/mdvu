# Decisions

`plan.md` との差異、および実装時に確定した事項を記録する。

## Toolchain

- Rust 2024 Edition。開発環境の stable は 1.97.1。
- `rust-toolchain.toml` は `stable` 追従とし、MSRV は宣言しない（`plan.md` 15章に従い MSRV job を追加しない）。

## Dependency baseline

`plan.md` 10章の想定 version line を、同一著者の既存実装 `halkn/docsail` 0.4.0 で実際に組み合わせが成立している構成に合わせて確定した。

| Crate | 採用 | 備考 |
|---|---|---|
| `ratatui` | 0.30.2 | `default-features = false`, `crossterm` のみ |
| `crossterm` | 0.29.0 | |
| `pulldown-cmark` | 0.13.4 | `default-features = false` |
| `merman` | 0.7.0 | `default-features = false`, `ascii` のみ |
| `unicode-width` | 0.2.2 | |

`merman` の feature は docsail と異なる。docsail は画像描画のため `raster` を使うが、`mdvu` は
`plan.md` 6.1 の text-only 方針に従い `ascii` のみを有効化し、`render` / `raster` および
image 関連 feature を持ち込まない。`ascii` feature は 0.7.0 に実在し、`merman-ascii` のみを
追加で引き込むことを `cargo check` で確認した。

### merman の推移的依存

`merman-core` が HTML label 処理のため `lol_html` / `selectors` / `cssparser` / `icu_*` を、
日付処理のため `chrono` を引き込む。この結果、依存グラフ全体は 199 crate になった
（`plan.md` 10章の「想定外に重い dependency は記録する」に該当）。

許容した理由:

- すべて pure Rust であり、`plan.md` 1.3-5 が禁じる Node.js / Chromium / WebView /
  外部コマンドの runtime 依存は発生しない。
- JavaScript engine は含まれない（`boa` / `v8` / `quickjs` はグラフに存在しない。
  `ryu-js` は ECMAScript 互換の数値フォーマット専用 crate）。
- `merman` は `plan.md` が指定する Mermaid 実装であり、代替は非目標。

ビルド時間が問題になった場合も、`merman` を差し替えるのではなく `diagram/mermaid.rs` の
adapter 境界（`plan.md` 6.3）内で対処する。

### 重複依存

`cargo tree -d` は v0.1 時点で 26 件、v0.2 で 30 件の重複を報告する。いずれも推移的依存（`merman` 系と dev 依存の
`insta` / `assert_cmd` 系）に由来し、`hashbrown` / `phf_shared` / `itertools` などの
メジャーバージョン差である。直接依存の選択で解消できるものは無いため、そのままとする。

## Document IR

`plan.md` 7.2 の IR は「例」として提示されたものであり、以下を変更した。

- `Block::Image` は設けない。CommonMark では画像は inline 要素であり、段落と独立した
  block にすると source range と wrap 処理が二重化する。`Inline::Image` として保持し、
  layout 側で `[image: ...]` / `[attachment: ...]` の placeholder に変換する。
- `Block::Diagram` に `rendered` / `warnings` / `error` / `unsupported` を追加した。
  理由は次節。

## Mermaid の描画タイミング

`plan.md` 4.4 は resize 時に「新しい diagram width で Mermaid を再 render する」と
規定するが、`merman` 0.7 の `AsciiRenderOptions` は幅オプションを持たず、出力幅は
内容から決まる。したがって幅を渡す先が無く、再 render しても結果は同一になる。

このため diagram は **parse 直後・layout 前に一度だけ** render し、結果を
`DiagramBlock::rendered` に保持する。resize では再 render しない。

この方針は `plan.md` 6.4 の「`merman` が width option を持つ場合は渡す」に適合し、
12章「Ratatui frame ごとに `merman` を呼ばない」も満たす（render は frame loop の
完全に外側で起きる）。将来 `merman` が幅指定に対応した場合は、`diagram::resolve` を
resize 経路から呼び直すだけで対応できる。

## CLI の非互換組み合わせ

`plan.md` 3.3 が列挙する不正な組み合わせのうち、`--plain` と `--color always` は
usage error（exit 2）にする。一方 `--plain --color never` は矛盾しないため受理する。
判定には clap の `ValueSource` を使い、`--color` が明示指定された場合だけ衝突とみなす
（既定値との衝突は起こさない）。

## 未終端の `:::` container

markdown-it 系の実装では、閉じられていない container は文書末尾まで伸びる。これをそのまま
採用すると、`::: mermaid` の閉じ忘れ 1 個で以降の文書全体が diagram body に吸収され、
viewer としては最も見たいもの（残りの本文）が読めなくなる。

そのため未終端 container の body は **最初の空行で打ち切る**。`plan.md` 7.5 の
「Malformed extension は消さずに、読める source fallback へする」を満たしつつ、
後続の Markdown が通常どおり描画される。打ち切りは diagnostic として報告する。

あわせて、container の閉じマーカー探索は code fence を認識する。fence 内の `:::` は
リテラルであり closer ではない。

## Release automation（MVP スコープ外）

`plan.md` 2.2 と 15章は Release automation と Release artifact 自動生成を MVP 非目標として
除外している。ユーザーの明示的な依頼によりスコープを広げ、`.github/workflows/release.yml`
と `docs/releasing.md` を追加した。

方針は非目標の趣旨をできるだけ維持する形で決めた。

- crates.io publish は行わない（2.2 の「crates.io公開」除外を維持）。配布は GitHub
  Releases のバイナリのみ。
- `cargo-dist` は導入しない。`plan.md` 18章が v0.3 候補として挙げており、MVP の依存を
  増やさない方針（10章）にも合わない。GitHub Actions と `gh` CLI だけで完結させた。
- 配布 target は 4 つ（Linux / macOS の x86_64 と aarch64）。すべて native runner で
  ビルドするため、クロスコンパイル用の toolchain や `cross` を持ち込まない。Windows は
  2.1 のとおり CI の compile / test 対象に留め、バイナリは配布しない。
- タグと `Cargo.toml` の version 不一致は release job で即座に失敗させる。

### `halkn/docsail` の CI/CD との差分

docsail のパイプラインは v0.4.0 まで実運用され、修正コミットを経ている。実績のある
選択はそちらへ合わせ、`plan.md` が要求する箇所だけ mdvu 側を強くしている。

docsail に合わせたもの:

- `actions/upload-artifact@v7` / `actions/download-artifact@v8` / `actions/checkout@v6`。
- checksum は publish job で `sha256sum *.tar.gz` を一度だけ実行する。glob に `./` を
  付けると記録されるパスがダウンロード後のファイル名と一致せず `sha256sum -c` が
  落ちるため、`./` は付けない。
- Intel macOS を native ビルドしない。docsail は `9f3299c fix: drop Intel macOS release
  build` で `macos-13` を削除している。mdvu では target 自体は維持し、arm64 runner 上の
  クロスビルドに切り替えた（smoke test はこの target のみ行わない）。
- CI に `permissions: contents: read` を置く。

mdvu 側を強くしたもの:

- CI の OS matrix と `--all-features`（`plan.md` 14.5 / 15章の要求）。docsail は
  ubuntu のみ・feature 指定なし。
- release 前の version 一致検証と品質ゲート実行。docsail はタグを打てば無検証で公開する。
- `gh release create --verify-tag`。
- アーカイブへの README / LICENSE 同梱。MIT ライセンスは配布物に添付が必要。
- `Swatinem/rust-cache`。

意図的に採用しなかったもの:

- CI 全体への `RUSTFLAGS: -D warnings`。`RUSTFLAGS` は依存 crate のコンパイルにも適用
  されるため、199 crate のいずれかが警告を出すと CI が落ちる。lint gate は
  `cargo clippy -- -D warnings` に任せる。docsail も設定していない。

## Table の intrinsic minimum width

当初 column の最小幅を `wrap_spans(cell, 1)` の結果から求めていたが、この関数は幅に
収まらない run を強制分割するため、最小幅が常に 1〜2 に潰れていた。結果として
`plan.md` 8.6 の vertical fallback がほぼ到達不能になり、代わりに `Ter` / `m` のように
語中で折れた 3 桁幅の表が出ていた。

`wrap::min_unbreakable_width` を追加し、「break opportunity を持たない最長の run の幅」を
最小幅とした。これにより語中分割が起きなくなり、本当に収まらない場合だけ vertical
fallback へ落ちる。

## 禁則処理

当初は行頭禁則 9 文字 / 行末禁則 7 文字を `ChunkKind::Closing` / `Opening` として持ち、
`break_line` が組み上がった行に対して追い出し・追い込みを適用していた。この形には
2 つの問題があった。

- JIS X 4051 の主要クラス（小書き仮名・長音・区切り約物・中点類・繰返し記号）が丸ごと
  欠けていた。`ちょっと` は `ち` / `ょっと` に折れていた
- 2 つの規則が互いを打ち消していた。行末禁則の追い出しを先に判定した後、行頭禁則の
  追い込みで直前 chunk を引き下ろすが、その結果に対して行末禁則を再判定しないため、
  `あいうえおかきく「け」` を幅 20 で折ると `「` が行末に露出した

**後処理をやめ、`chunk()` で結合する形に変えた。** 行頭禁則文字は直前の chunk へ吸収し、
行末禁則文字は直後の cell を自分の chunk へ引き込む。禁則違反の位置に break opportunity
が生成されないので、規則の適用順という概念自体が消える。`break_line` と `wrap_spans` の
`carried` 機構は不要になり削除した。

収束ループで直すこともできたが採らなかった。入れ子リスト内では実効幅が 1 まで縮む
（`layout/document.rs` の `saturating_sub(marker).max(1)` 連鎖）ため、ループの停止性を
別途保証する必要が出る。結合方式なら単一パスで完結する。
md-render.nvim も同じ結論に至っている（`lua/md-render/wrap.lua`、
"preventing cascading 追い出し issues in wrap_words"）。

### ASCII 半角約物を対象外にした理由

対象は全角約物・半角カナ・繰返し記号・小書き仮名・長音に限った。mdvu の Latin 折り返しは
空白区切りなので、ASCII 約物の禁則は元々ほぼ no-op である。一方 ASCII を分類すると
語中に break opportunity ができ、`min_unbreakable_width` が上記セクションの意図に反して
小さい値を返すようになる。実利が無く副作用だけがあるため入れない。

### 幅よりも禁則を優先する

`split_oversized` は合法な分割位置が見つからない場合、幅を超えることを許す（追い込み）。
禁則違反より幅超過の方が受け入れやすいという判断で、md-render.nvim も同じ挙動を取る。
発生するのは極端に狭い幅か禁則文字の長い連続だけで、通常の文書では起きない。

そのため `tests/render.rs` の不変条件を「幅以内」から「幅を超えるのは、幅の位置での
分割が禁則違反になる場合だけ」へ緩めた。mdvu は lib target を持たず統合テストから
文字集合を import できないため、判定用のテーブルはテスト側に複製し、複製である旨を
コメントに書いてある。

## v0.2 のスコープ

`plan.md` 18章の v0.2 候補のうち、config file / OSC 8 hyperlink / syntax highlighting /
TOC navigation / file watch を実装した。以下は実装しない。

- **Azure diagnostic 拡張**: `plan.md` 5.4 は Mermaid 互換ルールを「MVP で最低限検出するもの」
  として 4 件に限り、data-driven に隔離して追加しやすくすると定めていた。その拡張が候補だったが、
  Azure DevOps の Mermaid サポート仕様は変更が多く、ルールの追随コストが実利に見合わない。
  `diagram/azure_compat.rs` の `RULES` は構造をそのまま維持し、必要が生じた時点で追加する。
- **Link open**: `open` / `xdg-open` の起動が必要で、`plan.md` 11.3 と 17章 DoD の
  「External command 実行なし」を破る。この不変条件は維持し、リンクを開くのは OSC 8 経由で
  端末に委ねる。

## OSC 8 hyperlink を stdout 限定にした理由

ratatui 0.30 の `Cell` はハイパーリンク属性を持たない。cell の symbol へエスケープ列を
埋め込めば描画自体は可能だが、ratatui は差分描画のため、リンクを開いたセルだけが再描画された
場合に閉じ側が出力されず、以降の画面全体がリンク扱いになる。ratatui の内部実装に依存する
壊れ方であり、割に合わない。

そのため OSC 8 は `output/ansi.rs`（`--no-pager` / パイプ / `fzf --preview`）だけで出力し、
pager 内のリンクは従来どおり style 付きテキストとする。制約は README の Known limitations に
記載した。

### 下線の意味

`StyleRole::Link` から下線を外し、**`RenderedSpan::link` を持つ span**（= 宛先が
`http` / `https`）にだけ backend 側で下線を付ける。`Link` ロールは色だけを持つ。

理由は、リンクとして style される範囲（相対パス・`.attachments`・`#123`・`@alias`）が
実 URL より広いため。すべて同じ下線付きだと、どれが実際の URL なのか区別できない。

当初は「下線 = この端末で開ける」として OSC 8 を出した span だけに付けたが、pager は
OSC 8 を出さないため、主要な UI である pager で下線が一切出なくなった。そのため意味を
「下線 = 宛先が実 URL」に統一し、`output/ansi.rs` と `pager/view.rs` の両方で同じ条件で
付ける。`--hyperlinks` の有無で見た目が変わることもなくなった。OSC 8 を出す経路では、
下線が付いた語がそのまま端末で開ける語になる。

リンク化するのは `http` / `https` のみとした。相対パス・`.attachments`・`mailto:` などは
`plan.md` 8.7 の「target は読まない」に従い表示のみとする。加えて、制御文字を含む dest と
2083 バイト超の dest はリンク化しない。前者はエスケープ列を閉じて任意の OSC を注入できるため、
後者は正当な URL よりも壊れた入力である可能性が高いため。

## Syntax highlighting

`syntect` は **パーサとしてのみ** 使い、syntect のテーマは使わない。`ParseState` +
`ScopeStack` で得たスコープを `StyleRole::Syntax(SyntaxKind)` の 7 種へ写像し、色は
`layout/theme.rs` が決める。

この形にした理由は、既存の `Color`（16 色列挙）を truecolor へ広げずに済むこと、dark / light
両テーマと plain backend、snapshot の安定性がそのまま保たれること、そして `merman` と同じく
外部 crate を 1 module（`layout/highlight.rs`）に隔離できることによる。

- feature は `parsing` / `default-syntaxes` / `regex-fancy` のみ。C 依存の `onig` を避け、
  `default-themes` と `html` は持ち込まない。
- `SyntaxSet` は `OnceLock` で遅延初期化する。code block が無い文書は読み込まない。
- ハイライトはタブ展開後の文字列に対して行う。展開前に行うと列がずれる。
- スコープ解決は内側から外側へ走査するが、`punctuation.definition` は透過させる。これがないと
  コメントの `//` や文字列の引用符が comment / string ではなく punctuation になる。
- 分類されなかった範囲は `StyleRole::Code`（一色の黄）ではなく `Normal` にする。前者だと
  識別子がすべて色付きになり、分類できたトークンが埋もれる。

依存は 199 → 286 crate に増えた。増分は syntect の syntax 定義読み込み（`bincode` /
`flate2` / `serde_yaml` など）と `toml` / `serde` / `notify` による。

## Config file のスコープ

既存フラグの既定値上書きだけに限定した。`plan.md` 2.2 は Config system と User-defined theme を
非目標としており、既定値の外部化はその趣旨を最小限だけ緩めるものとして受け入れられるが、
テーマ色や keymap まで開くと意味論が増え、`plan.md` 19章が禁じる「user-visible CLI flag の
勝手な変更」に近い領域へ踏み込む。

- 値の解析は clap の `ValueEnum::from_str` を使う。serde derive で書き直すと、受理される綴りが
  `--help` と二重管理になる。
- `serde(deny_unknown_fields)` を付ける。typo が黙って無効になる方が実害が大きい。
- 探索順は `MDVU_CONFIG` → `$XDG_CONFIG_HOME/mdvu/config.toml` → `~/.config/mdvu/config.toml`。
  macOS でも `~/.config` に統一し、`dirs` 系の依存を増やさない。
- `MDVU_CONFIG` が空文字なら読み込まない。テストはこれを使い、実行環境のホームに依存しない。
- CLI が明示指定されたかの判定は、既存の `--plain` / `--color` 衝突判定と同じ clap の
  `ValueSource` を使う。

## File watch

`notify` の `RecommendedWatcher` をイベントを `std::sync::mpsc` で受ける形で使い、既存の
`crossterm::event::poll`（250ms）ループのタイムアウト側で回収する。async runtime は追加しない
（`plan.md` 2.2 / 12章）。再読込は frame 描画の外側で起き、起動時とまったく同じ経路
（`input::load` → `flavor::parse` → `diagram::resolve` → `layout_document`）を通る。

- **監視対象はファイルではなく親ディレクトリ**。エディタや Coding Agent の atomic save
  （一時ファイル → rename）は inode を差し替えるため、ファイル自体への watch は静かに外れる。
  イベントは file name で絞り込むので、同じディレクトリの他のファイルは無視される。
- イベント種別は `EventKind::Access` だけを捨て、他はすべて変更として扱う。backend ごとに
  分類の粒度が違い、取りこぼしより余分な再読込の方が安い。
- デバウンスは 100ms。`notify-debouncer-*` は追加せず、`Instant` を引数に取る純粋な
  `Debounce` として持ち、タイマ非依存にテストする。
- 読み込み失敗（一時的な truncate、消失、非 UTF-8 への差し替え）は fatal にせず、status bar に
  出して直前の描画を保持する。
- `--watch` は stdin と `--no-pager` との併用を usage error にする。ただし config の
  `watch = true` は適用できない状況では黙って無視する。設定を書いた人が `mdvu -` を使うたびに
  usage error になるのは筋が悪い。

### sandbox 内で検証できない部分

`pager/watch.rs` の `a_write_to_the_file_is_noticed` と `an_atomic_save_is_noticed` は、
実際にファイルを書き換えて `RecommendedWatcher`（macOS では FSEvents）のイベントを待つ。
開発時の sandbox では FSEvents のイベントが 1 件も届かず、この 2 件が失敗する。

sandbox の制約であってコードの問題ではないことは、`Watch` の watcher を `PollWatcher`
（OS 通知を使わない stat ポーリング）へ差し替えると同じ 2 件が通ることで確認した。
ディレクトリ監視・file name フィルタ・デバウンスはいずれも正しく動いている。

出荷するのは `RecommendedWatcher` とする。監視対象は 1 ファイルなのでポーリングでも実害は
小さいが、待機中の wakeup が無く反応も即時である通知ベースの方が pager に適している。
2 件は無効化せず CI で検証する。実際に macOS CI で通ることを確認した。

なお FSEvents は、watch 開始の直前に起きた書き込みを起動後に報告することがある。テストは
`Watch::new` の直後にファイルを作るため、この積み残しを捨ててから本題の操作を行う
（`settle`）。捨てないと、テストが起こしていない変更で reload が観測される。実運用では
文書を開いてから保存されるまでに間があるので問題にならない。

## 見出しオーバーレイ

`markdown::model::headings()` を使うため flavor に依存せず、Azure の `[[_TOC_]]` 展開とは
独立に動く。見出しの source 行は既存の `pager::state::rendered_line_for_source` で rendered 行へ
解決し、resize / 再読込のたびに解決し直す。

pager の対話部分は自動テストできないため、状態遷移は `state.rs` / `event.rs` の純粋な unit test で
固め、描画自体は ratatui の `TestBackend` で 1 フレーム描いて検証する。

## Windows

CI で build と unit test を実行するが、Windows terminal 上での pager 対話動作は
未検証である。この制約は README の Known limitations に記載した。
