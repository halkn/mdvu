# mdvu

A fast terminal Markdown viewer for GFM and Azure DevOps Wiki Markdown, built in Rust.

`mdvu` renders one Markdown file — or stdin — as styled terminal text, including
Mermaid diagrams drawn as Unicode or ASCII art. It is built for reviewing a
document a coding agent just changed, without leaving the terminal and without a
browser, Node.js or any external command.

## Why `mdvu`

- **One document at a time.** No file tree, no workspace, no navigation history.
  You point it at a file, read it, and quit.
- **Azure DevOps Wiki as a first-class flavor.** `[[_TOC_]]`, `::: mermaid`,
  `<details>`, work item references and attachments are understood, not shown as
  stray syntax.
- **Mermaid without a runtime.** Diagrams are parsed and drawn by
  [`merman`](https://docs.rs/merman/) as terminal text. There is no Chromium, no
  `mmdc`, no image protocol.
- **Japanese text is not an afterthought.** Wrapping uses Unicode display width
  and grapheme boundaries, with best-effort kinsoku so `。` does not start a line.
- **Composes with your tools.** `fzf`, `fd`, `rg` and Git stay external.

> `mdvu` is more review-oriented than Glow, more Markdown-specific than mcat, and
> CLI-native rather than editor-native like md-render.nvim.

## Installation

### From a release

Prebuilt binaries for Linux and macOS are attached to each
[release](https://github.com/halkn/mdvu/releases).

```console
tag=v0.1.0
target=aarch64-apple-darwin   # or x86_64-apple-darwin, {x86_64,aarch64}-unknown-linux-gnu
curl -fsSLO "https://github.com/halkn/mdvu/releases/download/$tag/mdvu-${tag#v}-$target.tar.gz"
tar xzf "mdvu-${tag#v}-$target.tar.gz"
install "mdvu-${tag#v}-$target/mdvu" ~/.local/bin/
```

Each release also carries a `SHA256SUMS` file. `mdvu` is not published to
crates.io, and Windows binaries are not distributed.

### From source

Requires a stable Rust toolchain (2024 edition).

```console
git clone https://github.com/halkn/mdvu
cd mdvu
cargo install --path .
```

## Usage

```text
mdvu [OPTIONS] [FILE]
```

`FILE` is a Markdown file, `-` for stdin, or omitted when stdin is a pipe.

```console
mdvu README.md                       # interactive pager
mdvu docs/architecture.md --line 143 # open near a source line
git show HEAD:docs/design.md | mdvu -
mdvu --no-pager --plain doc.md       # unstyled text to stdout
```

### Options

| Option | Description |
|:-------|:------------|
| `-p, --pager` | Force the interactive pager |
| `--no-pager` | Render to stdout without entering the alternate screen |
| `-w, --width <COLUMNS>` | Override the rendering width |
| `-l, --line <LINE>` | Open near the given 1-based source line |
| `--flavor <FLAVOR>` | `gfm` or `azure-devops` (default: `azure-devops`) |
| `--mermaid <MODE>` | `unicode`, `ascii`, `source`, `off` (default: `unicode`) |
| `--theme <THEME>` | `auto`, `dark`, `light` (default: `auto`) |
| `--color <WHEN>` | `auto`, `always`, `never` (default: `auto`) |
| `--plain` | Alias for `--color never` |

Without `--pager` or `--no-pager`, `mdvu` opens the pager when stdout is a
terminal and writes to stdout otherwise. `--color auto` honours `NO_COLOR` and
disables ANSI when stdout is not a terminal. `--theme auto` uses the `COLORFGBG`
hint when present and falls back to dark; it never issues a blocking terminal
query.

Exit codes: `0` success, `1` a fatal input, decode, terminal or output error,
`2` a usage error. A Mermaid diagram that fails to render is never fatal.

## Pager keys

| Key | Action |
|:----|:-------|
| `j`, `Down` | Down one line |
| `k`, `Up` | Up one line |
| `Ctrl-d` / `Ctrl-u` | Half screen down / up |
| `Space`, `PageDown` | One screen down |
| `b`, `PageUp` | One screen up |
| `g`, `Home` | Top of the document |
| `G`, `End` | End of the document |
| `h`, `Left` / `l`, `Right` | Scroll horizontally |
| `0` | Reset horizontal scroll |
| `/` | Search, `Enter` to confirm, `Esc` to cancel |
| `n` / `N` | Next / previous match |
| `q`, `Esc` | Quit |

Search runs over the rendered text, is case-insensitive, highlights every match
on screen and cycles with `n` and `N`. An empty query keeps the previous one.
Resizing re-runs layout and keeps the source line that was at the top of the
viewport.

## Integration

### `fzf` preview

```console
fd --type f --extension md |
  fzf --preview 'mdvu --no-pager --color always --width "$FZF_PREVIEW_COLUMNS" {}'
```

`--color always` is required: `fzf` captures the preview, so `auto` would
correctly decide the output is not a terminal and drop the styling.

### Reviewing changed Markdown

```console
git diff --name-only --diff-filter=ACMR -- '*.md' |
  fzf --preview 'mdvu --no-pager --color always --width "$FZF_PREVIEW_COLUMNS" {}'
```

`mdvu` renders the document as it now stands. Reading the diff itself is the job
of `git diff`, `delta` or a dedicated hunk tool — `mdvu` deliberately has no Git
integration.

## Markdown support

Rendered: ATX headings, paragraphs, bold, italic, strikethrough, inline code,
fenced and indented code blocks, ordered and unordered lists, nested lists, task
lists, block quotes, nested quotes, horizontal rules, GFM tables, links,
autolinks, images as text placeholders, footnotes, and hard and soft breaks.

Tables get column widths from intrinsic minimum and preferred widths, measured in
display columns. When even the minimum widths do not fit, the table becomes a
vertical list rather than a broken grid. Cell contents are never silently
truncated.

Code blocks are not syntax highlighted and are not re-wrapped; scroll them
horizontally instead. Tabs expand to four-column tab stops.

Raw HTML is kept as literal text. Nothing in a document is ever executed:
no JavaScript, no iframes, no network requests, no subprocesses.

## Azure DevOps Wiki support

Active under the default `--flavor azure-devops`.

| Syntax | Rendering |
|:-------|:----------|
| `[[_TOC_]]` | Table of contents built from the ATX headings |
| Second `[[_TOC_]]` | Shown as ignored, matching Azure DevOps |
| `[[_TOSP_]]` | `Child pages unavailable in single-file mode` |
| `::: mermaid` | Rendered diagram |
| ` ```mermaid ` | Rendered diagram |
| `<details><summary>` | Expanded, with a border |
| `#123` | Styled work item reference, no API lookup |
| `@alias` | Styled mention, no identity lookup |
| `<br/>` in a table cell | Line break inside the cell |
| `.attachments/...` | Image or attachment placeholder with its path |
| `::: video` | Unsupported media placeholder |
| `::: query-table` | Query placeholder, with the query id when present |
| `$...$`, `$$...$$` | Shown as math source; KaTeX is not evaluated |
| Unknown `::: block` | Placeholder that keeps the original body |

The `[[_TOC_]]` macro is case-sensitive, only the first occurrence expands, and
only `#` headings are collected. Azure syntax inside a code fence stays literal.
An unterminated `:::` container is reported and its body is preserved.

Under `--flavor gfm` all of the above is disabled, and Azure macros are treated
as ordinary Markdown text.

## Mermaid

`mdvu` renders Mermaid syntax to Unicode or ASCII terminal text with `merman`.
Diagrams are rendered once, before layout, so the frame loop never calls the
renderer.

- `--mermaid unicode` (default) uses box drawing characters.
- `--mermaid ascii` restricts output to 7-bit ASCII.
- `--mermaid source` shows the Mermaid source in a labelled block.
- `--mermaid off` shows a one-line marker.

`graph`, `sequenceDiagram`, `classDiagram` and `erDiagram` render as diagrams.
Other families, including `stateDiagram-v2`, fall back to their source with a
note. A diagram that fails to parse falls back to its source with a short,
normalised error; the rest of the document still renders and the process still
exits `0`.

Under the Azure flavor, `mdvu` warns about four documented Azure DevOps
incompatibilities: the `flowchart` root keyword, long arrows such as `---->`,
Font Awesome icons, and HTML tags inside labels. A flagged diagram still renders.

## Comparison

| Tool | Focus | How `mdvu` differs |
|:-----|:------|:-------------------|
| [Glow](https://github.com/charmbracelet/glow) | General Markdown reader with a file browser | `mdvu` is review-oriented, single-file, source-line aware and Azure DevOps aware |
| [mcat](https://github.com/Skardyy/mcat) | Many file formats in the terminal | `mdvu` is Markdown only; no PDF, DOCX, HTML, image or video input |
| [md-render.nvim](https://github.com/delphinus/md-render.nvim) | High quality Markdown inside Neovim | `mdvu` is a standalone CLI with no editor dependency |
| [DocSail](https://github.com/halkn/docsail) | Markdown workspace viewer with a file tree | `mdvu` shows one file or stdin and has no workspace navigation |

## Known limitations

- Not a full GFM browser renderer and not pixel-identical to Azure DevOps.
- Not fully Mermaid.js compatible. `merman` covers a subset of diagram families,
  and its text layout measures labels by character count, so diagrams with
  Japanese labels can have misaligned borders even though the labels are correct.
- The Azure DevOps compatibility check covers four known rules only. It is not a
  validator; a clean run does not mean Azure DevOps will accept the diagram.
- Kinsoku handling is best effort and does not implement JIS X 4051.
- No syntax highlighting, no link opening, no OSC 8 hyperlinks, no mouse, no
  file watching, no configuration file and no user themes.
- Linux and macOS are the primary targets. Windows is built and unit-tested in
  CI, but the pager has not been verified interactively on a Windows terminal.

## Development

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo tree -d
```

Golden rendering tests drive the binary end to end and compare snapshots with
[`insta`](https://insta.rs). Review changes with `cargo insta review`, or
regenerate with `INSTA_UPDATE=always cargo test --test render`.

Design decisions and deviations from the original plan are recorded in
[`docs/decisions.md`](docs/decisions.md). The release procedure is in
[`docs/releasing.md`](docs/releasing.md).

## License

MIT. See [LICENSE](LICENSE).
