# mdvu

A fast terminal Markdown viewer for GFM and Azure DevOps Wiki Markdown, built in
Rust.

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
  [`merman`](https://docs.rs/merman/) as terminal text. There is no Chromium and
  no `mmdc`.
- **Local images, where the terminal can show them.** A PNG, JPEG, GIF or WebP
  next to the document is drawn inline with the kitty or iTerm2 graphics
  protocol. No decoder, no network, no external command.
- **Japanese text is not an afterthought.** Wrapping uses Unicode display width
  and grapheme boundaries, with JIS X 4051 kinsoku so `。`, `ー` and a small
  kana never start a line and `「` never ends one.
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
mdvu --watch notes.md                # follow the file while an agent edits it
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
| `--hyperlinks <WHEN>` | OSC 8 links in stdout output: `auto`, `always`, `never` (default: `auto`) |
| `--highlight <WHEN>` | Syntax highlighting for code blocks: `auto`, `never` (default: `auto`) |
| `--images <WHEN>` | Inline images: `auto`, `kitty`, `iterm2`, `never` (default: `auto`) |
| `--icons <SET>` | Glyphs for alerts, code fences and placeholders: `unicode`, `nerd` (default: `unicode`) |
| `--watch` | Re-render when the file changes on disk (pager only) |
| `--plain` | Alias for `--color never` |

Without `--pager` or `--no-pager`, `mdvu` opens the pager when stdout is a
terminal and writes to stdout otherwise. `--color auto` honours `NO_COLOR` and
disables ANSI when stdout is not a terminal. `--theme auto` uses the `COLORFGBG`
hint when present and falls back to dark; it never issues a blocking terminal
query.

`--hyperlinks` marks `http` and `https` link labels as OSC 8 terminal
hyperlinks, so a supporting terminal can open them. Other destinations —
relative paths, attachments, anchors and other schemes — are shown but never
linked, and `mdvu` itself never opens anything. Hyperlinks are escape sequences,
so `--plain` and `--color never` suppress them. `auto` emits them only when
stdout is a terminal; use `always` for a captured preview such as `fzf`.

Underline means "the destination is a real URL". Link labels are coloured, but
only `http` and `https` targets are underlined, so they stand out from a
relative path, a `#123` or an `@alias` styled the same way. The mark is the same
in the pager and on stdout; where hyperlinks are emitted, an underlined label is
also the one the terminal can open.

`--images` draws local images with a terminal graphics protocol; see
[Images](#images) below. `--icons nerd` swaps parts of the chrome for Nerd Font
glyphs; see [Icons](#icons).

`--watch` follows the file while something else edits it — a coding agent, or
your editor in another window — and re-renders on every save. The reading
position is kept: the source line at the top of the viewport stays there. A save
that is briefly unreadable is reported in the status bar and leaves the previous
rendering on screen. Watching needs a file and the pager, so it cannot be
combined with stdin or `--no-pager`.

Exit codes: `0` success, `1` a fatal input, decode, terminal or output error,
`2` a usage error. A Mermaid diagram that fails to render is never fatal.

### Configuration

`mdvu` reads `~/.config/mdvu/config.toml` if it exists. It only sets defaults
for the flags above; a flag given on the command line always wins. `MDVU_CONFIG`
overrides the path, and setting it to an empty string disables the file.

```toml
flavor = "gfm"
mermaid = "ascii"
theme = "dark"
color = "auto"
hyperlinks = "always"
highlight = "auto"
images = "auto"
icons = "unicode"
width = 100
watch = true
```

An unknown key or an invalid value is a usage error rather than something
silently ignored. `watch = true` is skipped where it cannot apply, such as when
reading stdin.

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
| `/` | Search as you type, `Enter` to confirm, `Esc` to cancel |
| `n` / `N` | Next / previous match |
| `t` | Heading list, `j` / `k` to select, `Enter` to jump, `Esc` to close |
| `?` | Key list, `j` / `k` to scroll, `Esc` to close |
| `q`, `Esc` | Quit |

Search runs over the rendered text, highlights every match on screen and cycles
with `n` and `N`, saying `wrapped` when it passes either end. The match being
visited is coloured, the rest are plain reversed, so which one the viewport
moved to is visible without counting. The status bar
shows which match is current, as `(3/12)`. An empty query keeps the previous
one. Case follows the query: a query in lower case matches any case, and one
capital makes the whole query case-sensitive.

The document follows the query as it is typed: every keystroke jumps to the
first match at or after where `/` was pressed, so a query can be judged before
confirming it. `Esc` puts back both the previous search and the reading
position; `Enter` keeps what is on screen. The prompt takes the readline keys —
`Ctrl-a` / `Ctrl-e`, `Ctrl-b` / `Ctrl-f`, arrows and `Home` / `End` to move,
`Ctrl-w`, `Ctrl-u`, `Ctrl-k`, `Ctrl-d` and `Backspace` to delete.

`?` opens the same key list inside the pager, so the bindings are readable
without leaving the document. The list in `src/pager/help.rs` is the source of
truth for it and for the table above.

`t` opens a list of the document's headings, preselecting the section on screen.
It works in both flavors and is independent of `[[_TOC_]]`.
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
lists, block quotes, nested quotes, alerts, horizontal rules, GFM tables, links,
autolinks, images, footnotes, and hard and soft breaks. An image standing alone
in its paragraph is drawn inline where the terminal supports it, and is a text
placeholder everywhere else; see [Images](#images).

A quote that opens with `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` or
`[!CAUTION]` is drawn as a labelled box in a colour matching its kind. The label
is a word, so no font beyond the box drawing characters is assumed and the kind
is still readable under `--plain`; `--icons nerd` puts a glyph in front of it.
Alerts work in both flavors — Azure DevOps Wiki uses the same syntax — and an
unrecognised kind such as `[!FOO]` stays an ordinary quote with its marker
intact.

Bullets change with nesting depth, cycling `•`, `◦` and `▪`.

Tables get column widths from intrinsic minimum and preferred widths, measured in
display columns. When even the minimum widths do not fit, the table becomes a
vertical list rather than a broken grid. Cell contents are never silently
truncated.

Fenced code blocks with a language are syntax highlighted. Tokens are classified
by [`syntect`](https://docs.rs/syntect/) and coloured from the same 16-colour
theme as the rest of the document, so both themes and every terminal work the
same way. An unknown language falls back to a uniform colour, and `--plain`
switches highlighting off entirely. Code blocks are not re-wrapped; scroll them
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

## Images

An image that is the whole of its paragraph is drawn inline when the terminal
supports it. Everything else about it stays as before: an image among words
keeps its `[image: ...]` placeholder, because a picture there would need a
multi-row box inside a wrapped line.

The file's own bytes are handed to the terminal, which decodes them, so PNG,
JPEG, GIF and WebP work and nothing is decoded inside `mdvu`. SVG is not drawn:
no terminal renders it.

`--images auto` reads the environment only — `TERM`, `TERM_PROGRAM`,
`KITTY_WINDOW_ID`, `KONSOLE_VERSION` — and never asks the terminal what it
supports, since that would mean writing to the tty and waiting for an answer.
kitty, Ghostty, WezTerm and Konsole get the kitty protocol; iTerm2 gets its own.
Inside `tmux`, `auto` stays off, because passthrough depends on the outer
terminal and the tmux version; `--images kitty` forces it. Images are escape
sequences, so `--plain`, `--color never` and `NO_COLOR` suppress them, and
`auto` requires stdout to be a terminal.

Which files may be read is deliberately narrow. `mdvu` opens nothing else in a
document: links, attachments and other destinations are shown, never followed.
An image is drawn only when all of the following hold, and otherwise keeps its
placeholder without an error:

- the document came from a file, so there is a directory to resolve against
- the destination is a local path with no URL scheme; `http`, `https` and `data`
  are never fetched, and `mdvu` makes no network requests
- the resolved path stays inside the content root, after both sides are
  canonicalised, so `../` leaving it and a symlink pointing outside are both
  refused. The content root is the nearest ancestor of the document holding a
  `.git` entry, and the document's own directory when there is none, so a wiki
  page reaches the `.attachments` directory at the repository root. A
  destination starting with `/` is read from the content root, the way Azure
  DevOps and GitHub resolve it, never from the filesystem root
- the extension is `png`, `jpg`, `jpeg`, `gif` or `webp`, and the file's leading
  bytes agree with it
- the file is at most 10 MiB

An image is scaled to fit the text width and capped at 20 rows, keeping its
aspect ratio. In the pager it is drawn only while it fits on screen whole: half
a picture over the status bar is worse than none, and neither protocol can crop
a placement without sending it again.

## Icons

`--icons nerd` draws part of the chrome with [Nerd Font](https://www.nerdfonts.com/)
glyphs: a glyph before an alert label, before a code fence's language, and in
place of the `[image: ...]` and `[attachment: ...]` words. Headings keep their
`#` markers and lists keep `•`, `◦` and `▪`; the level and the depth are the
information there, and a missing glyph would take it away.

The default is `unicode`, which renders exactly what `mdvu` rendered before this
option existed. Nothing is auto-detected: whether the terminal's font carries
these glyphs cannot be read from the environment, and asking the terminal would
mean writing to the tty and waiting for an answer, which `mdvu` never does. Turn
it on per run with `--icons nerd`, or once with `icons = "nerd"` in the
configuration file.

Glyphs are ordinary characters rather than escape sequences, so unlike images
and hyperlinks they are unaffected by `--plain`, `--color never` and `NO_COLOR`.
A terminal without a Nerd Font shows tofu in their place; the columns still line
up, because each glyph is measured as one display column.

## Comparison

| Tool | Focus | How `mdvu` differs |
|:-----|:------|:-------------------|
| [Glow](https://github.com/charmbracelet/glow) | General Markdown reader with a file browser | `mdvu` is review-oriented, single-file, source-line aware and Azure DevOps aware |
| [mcat](https://github.com/Skardyy/mcat) | Many file formats in the terminal | `mdvu` is Markdown only; no PDF, DOCX, HTML, image or video input, and images are drawn only as part of a document |
| [md-render.nvim](https://github.com/delphinus/md-render.nvim) | High quality Markdown inside Neovim | `mdvu` is a standalone CLI with no editor dependency |
| [DocSail](https://github.com/halkn/docsail) | Markdown workspace viewer with a file tree | `mdvu` shows one file or stdin and has no workspace navigation |

## Known limitations

- Not a full GFM browser renderer and not pixel-identical to Azure DevOps.
- Not fully Mermaid.js compatible. `merman` covers a subset of diagram families,
  and its text layout measures labels by character count, so diagrams with
  Japanese labels can have misaligned borders even though the labels are correct.
- The Azure DevOps compatibility check covers four known rules only. It is not a
  validator; a clean run does not mean Azure DevOps will accept the diagram.
- Inline images need kitty or iTerm2 graphics. Sixel is not implemented, so
  foot, xterm and Windows Terminal show placeholders. Detection is by
  environment variable, so an unlisted terminal needs `--images` naming the
  protocol.
- An image is drawn only when its paragraph holds nothing else, and only from
  the content root: the repository the document sits in, or its own directory
  outside one. SVG is never drawn.
- `--icons nerd` assumes each glyph occupies one column, which is what
  `mdvu` measures and what a Nerd Font Mono variant draws. A terminal that draws
  them two columns wide shifts the rest of that line by one.
- Kinsoku covers the JIS X 4051 CJK and halfwidth katakana classes. ASCII
  punctuation is excluded, and there is no phrase-level segmentation, so a line
  can still break in the middle of a Japanese word.
- Kinsoku wins over `--width`. A run with no legal break point inside it is kept
  whole and its line runs past the requested width rather than breaking where
  kinsoku forbids. This needs a very narrow width or a long run of punctuation
  to happen, but a captured preview such as `fzf` will show the longer line.
- OSC 8 hyperlinks are emitted by the stdout backend only. In the pager a URL is
  underlined but not clickable: `ratatui` cells carry no hyperlink attribute.
- `--watch` follows one file. A document that includes others is not tracked,
  because `mdvu` has no concept of includes.
- `mdvu` never opens a link or runs an external command. Clicking is the
  terminal's job.
- Syntax highlighting covers the languages shipped with `syntect` and classifies
  tokens into seven roles; it is not an editor-grade highlighter.
- No mouse support, no user-defined themes and no configurable key bindings.
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

Prebuilt binaries are published from `.github/workflows/release.yml` on a `v*`
tag; the tag must match the `version` in `Cargo.toml`.

Per-module design decisions — why kinsoku is applied while chunks are built, why
Mermaid renders once before layout, why `--watch` watches the parent directory —
live in [`.claude/rules/`](.claude/rules), next to the code they constrain.

## License

MIT. See [LICENSE](LICENSE).
