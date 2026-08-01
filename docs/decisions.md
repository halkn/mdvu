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

`cargo tree -d` は 26 件の重複を報告する。いずれも推移的依存（`merman` 系と dev 依存の
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

## Windows

CI で build と unit test を実行するが、Windows terminal 上での pager 対話動作は
未検証である。この制約は README の Known limitations に記載した。
