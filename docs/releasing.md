# Releasing

`mdvu` は GitHub Releases でビルド済みバイナリを配布する。crates.io へは公開しない
（`plan.md` 2.2）。パイプラインは `.github/workflows/release.yml`。

## 配布対象

| Target | Runner | Smoke test |
|:-------|:-------|:-----------|
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | あり |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | あり |
| `x86_64-apple-darwin` | `macos-latest` | なし（cross build） |
| `aarch64-apple-darwin` | `macos-latest` | あり |

Intel macOS だけは arm64 runner 上でクロスビルドする。Intel の macOS runner は
廃止済みで、`halkn/docsail` でも同じ理由で該当 target を落としている（`9f3299c`）。
Apple の toolchain は両アーキテクチャを標的にできるが、生成物を runner 上で実行できない
ため、この target のみ smoke test を行わない。

Windows は CI の build / test 対象だが、バイナリは配布しない。

## 手順

1. 作業ブランチを作る。

   ```console
   git switch -c release/v0.1.0
   ```

2. `Cargo.toml` の `version` を上げ、`Cargo.lock` を追随させる。

   ```console
   cargo update --workspace
   ```

3. ローカルで品質ゲートを通す。

   ```console
   cargo fmt --check
   cargo clippy --all-targets --all-features --locked -- -D warnings
   cargo test --all-features --locked
   ```

4. `Cargo.toml` と `Cargo.lock` をコミットし、PR を出してマージする。

5. マージ後の `main` にタグを打って push する。タグは `v` + `Cargo.toml` の version。

   ```console
   git switch main && git pull
   git tag -a v0.1.0 -m 'v0.1.0'
   git push origin v0.1.0
   ```

タグ push でパイプラインが起動する。

## パイプラインの動作

`verify` → `build` → `release` の順に実行する。

- **verify** — タグ名から `v` を除いた文字列が `Cargo.toml` の version と一致するか検証し、
  一致しなければ即座に失敗する。続けて fmt / clippy / test を `--locked` で実行する。
  ここが落ちた場合、成果物は一切生成されない。
- **build** — 4 target を並列ビルドする。native な 3 target では `--version` と
  最小レンダリングの smoke test を実行し、実行可能性まで確認する。
  `mdvu-<version>-<target>.tar.gz` に binary・README・LICENSE を収める。
- **release** — 全 artifact を集約して `sha256sum` で `SHA256SUMS` を作り、
  `--generate-notes` でリリースノートを生成して GitHub Release を作成する。
  `--verify-tag` により、リポジトリに存在しないタグでは作成しない。

同じタグで再実行した場合、既存の Release があれば `--clobber` で資産を差し替える。

## 検証

```console
gh release view v0.1.0
gh release download v0.1.0 --pattern 'mdvu-*-aarch64-apple-darwin.tar.gz' --pattern 'SHA256SUMS'
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`SHA256SUMS` は全 target の行を含むため、一部だけ取得した場合は `--ignore-missing` を
付ける。

## 失敗した場合

- **verify で version 不一致** — タグを削除し、`Cargo.toml` を直してから打ち直す。

  ```console
  git push origin :refs/tags/v0.1.0
  git tag -d v0.1.0
  ```

- **build が特定 target だけ失敗** — 修正をマージした後、同じタグを打ち直すか、
  `workflow_dispatch` に既存タグを渡して再実行する。Release があれば資産だけ差し替わる。

- **公開済み Release を取り消したい** — `gh release delete v0.1.0` で Release を消す。
  タグは別途削除する必要がある。ダウンロード済みの成果物は回収できない。

## 制約

- `ubuntu-24.04-arm` は public repository では無料だが、private repository では
  larger runner の契約が必要になる。private 化する場合は aarch64 Linux をクロス
  コンパイルに切り替えるか、対象から外す。
- バイナリへの署名・notarization は行わない。macOS では初回起動時に Gatekeeper の
  警告が出る。
