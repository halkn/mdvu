//! Golden rendering tests. The binary is driven end to end so the CLI wiring,
//! layout engine and output backends are all covered.

use assert_cmd::Command;

fn render(args: &[&str]) -> String {
    let output = Command::cargo_bin("mdvu")
        .expect("binary should be built")
        .args(args)
        // Keep the rendered surface independent of the developer's terminal
        // and of any configuration file they happen to have.
        .env_remove("COLORFGBG")
        .env_remove("NO_COLOR")
        .env("MDVU_CONFIG", "")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("output should be UTF-8")
}

fn plain(fixture: &str, width: &str) -> String {
    render(&["--no-pager", "--plain", "--width", width, fixture])
}

macro_rules! golden {
    ($name:ident, $fixture:expr, $width:expr) => {
        #[test]
        fn $name() {
            insta::assert_snapshot!(plain($fixture, $width));
        }
    };
}

/// Styled output, with the escapes made visible so a snapshot stays readable.
fn styled(fixture: &str, width: &str) -> String {
    render(&["--no-pager", "--color", "always", "--width", width, fixture]).replace('\x1b', "ESC")
}

golden!(gfm_showcase_40, "tests/fixtures/gfm/showcase.md", "40");
golden!(gfm_showcase_80, "tests/fixtures/gfm/showcase.md", "80");
golden!(gfm_showcase_120, "tests/fixtures/gfm/showcase.md", "120");
golden!(gfm_tables_40, "tests/fixtures/gfm/tables.md", "40");
golden!(gfm_tables_80, "tests/fixtures/gfm/tables.md", "80");
golden!(gfm_nested_40, "tests/fixtures/gfm/nested.md", "40");
golden!(gfm_nested_80, "tests/fixtures/gfm/nested.md", "80");
golden!(gfm_footnotes_40, "tests/fixtures/gfm/footnotes.md", "40");
golden!(gfm_wide_table_40, "tests/fixtures/gfm/wide-table.md", "40");
golden!(
    gfm_wide_table_120,
    "tests/fixtures/gfm/wide-table.md",
    "120"
);
golden!(
    japanese_showcase_40,
    "tests/fixtures/japanese/showcase.md",
    "40"
);
golden!(
    japanese_showcase_80,
    "tests/fixtures/japanese/showcase.md",
    "80"
);
golden!(
    japanese_punctuation_40,
    "tests/fixtures/japanese/punctuation.md",
    "40"
);
golden!(
    japanese_punctuation_80,
    "tests/fixtures/japanese/punctuation.md",
    "80"
);
golden!(
    japanese_tables_40,
    "tests/fixtures/japanese/tables.md",
    "40"
);
golden!(
    japanese_tables_80,
    "tests/fixtures/japanese/tables.md",
    "80"
);

fn plain_with(fixture: &str, width: &str, extra: &[&str]) -> String {
    let mut args = vec!["--no-pager", "--plain", "--width", width];
    args.extend_from_slice(extra);
    args.push(fixture);
    render(&args)
}

macro_rules! golden_with {
    ($name:ident, $fixture:expr, $width:expr, $extra:expr) => {
        #[test]
        fn $name() {
            insta::assert_snapshot!(plain_with($fixture, $width, &$extra));
        }
    };
}

golden_with!(
    azure_showcase_80,
    "tests/fixtures/azure-devops/showcase.md",
    "80",
    ["--flavor", "azure-devops"]
);
golden_with!(
    azure_showcase_as_gfm_80,
    "tests/fixtures/azure-devops/showcase.md",
    "80",
    ["--flavor", "gfm"]
);
golden_with!(
    azure_toc_80,
    "tests/fixtures/azure-devops/toc.md",
    "80",
    ["--flavor", "azure-devops"]
);
golden_with!(
    azure_details_80,
    "tests/fixtures/azure-devops/details.md",
    "80",
    ["--flavor", "azure-devops"]
);
golden_with!(
    azure_attachments_80,
    "tests/fixtures/azure-devops/attachments.md",
    "80",
    ["--flavor", "azure-devops"]
);
golden_with!(
    azure_containers_80,
    "tests/fixtures/azure-devops/containers.md",
    "80",
    ["--flavor", "azure-devops"]
);
golden_with!(
    azure_malformed_80,
    "tests/fixtures/azure-devops/malformed.md",
    "80",
    ["--flavor", "azure-devops"]
);

golden!(
    mermaid_azure_graph_80,
    "tests/fixtures/mermaid/azure-graph.md",
    "80"
);
golden!(
    mermaid_flowchart_warning_80,
    "tests/fixtures/mermaid/flowchart-warning.md",
    "80"
);
golden!(
    mermaid_sequence_80,
    "tests/fixtures/mermaid/sequence.md",
    "80"
);
golden!(mermaid_er_80, "tests/fixtures/mermaid/er.md", "80");
golden!(mermaid_class_80, "tests/fixtures/mermaid/class.md", "80");
golden!(mermaid_state_80, "tests/fixtures/mermaid/state.md", "80");
golden!(
    mermaid_japanese_80,
    "tests/fixtures/mermaid/japanese.md",
    "80"
);
golden!(
    mermaid_invalid_80,
    "tests/fixtures/mermaid/invalid.md",
    "80"
);
golden!(
    mermaid_unsupported_80,
    "tests/fixtures/mermaid/unsupported.md",
    "80"
);
golden!(
    mermaid_azure_graph_40,
    "tests/fixtures/mermaid/azure-graph.md",
    "40"
);
golden!(
    mermaid_azure_graph_120,
    "tests/fixtures/mermaid/azure-graph.md",
    "120"
);

golden_with!(
    mermaid_mode_ascii,
    "tests/fixtures/mermaid/sequence.md",
    "80",
    ["--mermaid", "ascii"]
);
golden_with!(
    mermaid_mode_source,
    "tests/fixtures/mermaid/sequence.md",
    "80",
    ["--mermaid", "source"]
);
golden_with!(
    mermaid_mode_off,
    "tests/fixtures/mermaid/sequence.md",
    "80",
    ["--mermaid", "off"]
);

#[test]
fn ascii_mode_emits_no_wide_drawing_characters() {
    let out = plain_with(
        "tests/fixtures/mermaid/azure-graph.md",
        "80",
        &["--mermaid", "ascii"],
    );
    let diagram: String = out
        .lines()
        .filter(|l| l.starts_with("│ "))
        .map(|l| l.trim_start_matches("│ "))
        .collect();
    assert!(diagram.is_ascii(), "ASCII mode leaked: {diagram}");
}

#[test]
fn a_failed_diagram_keeps_its_source_and_the_rest_of_the_document() {
    let out = plain("tests/fixtures/mermaid/invalid.md", "80");
    assert!(out.contains("graph LR"), "source was dropped");
    assert!(out.contains("Text after the diagram still renders."));
}

#[test]
fn gfm_flavor_leaves_azure_syntax_alone() {
    let out = plain_with(
        "tests/fixtures/azure-devops/showcase.md",
        "80",
        &["--flavor", "gfm"],
    );
    // `_TOC_` is ordinary CommonMark emphasis here, so the macro survives as
    // text rather than being expanded into a table of contents.
    assert!(out.contains("[[TOC]]"), "the macro should not be expanded");
    assert!(!out.contains("Contents"));
    assert!(!out.contains("Child pages unavailable"));
    assert!(!out.contains("[video]"));
}

#[test]
fn plain_output_contains_no_ansi_escapes() {
    for fixture in [
        "tests/fixtures/gfm/showcase.md",
        "tests/fixtures/japanese/showcase.md",
    ] {
        for width in ["40", "80", "120"] {
            assert!(
                !plain(fixture, width).contains('\x1b'),
                "{fixture} at width {width} leaked an escape sequence"
            );
        }
    }
}

#[test]
fn color_always_emits_ansi_even_when_captured() {
    let out = render(&[
        "--no-pager",
        "--color",
        "always",
        "--width",
        "80",
        "tests/fixtures/gfm/showcase.md",
    ]);
    assert!(out.contains('\x1b'), "expected ANSI escapes");
    assert!(out.contains("\x1b[0m"), "expected explicit resets");
}

#[test]
fn color_never_matches_plain() {
    let never = render(&[
        "--no-pager",
        "--color",
        "never",
        "--width",
        "80",
        "tests/fixtures/gfm/showcase.md",
    ]);
    assert_eq!(never, plain("tests/fixtures/gfm/showcase.md", "80"));
}

#[test]
fn no_color_env_disables_ansi_under_auto() {
    let out = Command::cargo_bin("mdvu")
        .expect("binary should be built")
        .args([
            "--no-pager",
            "--width",
            "80",
            "tests/fixtures/gfm/showcase.md",
        ])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8_lossy(&out).contains('\x1b'));
}

/// Mirrors the JIS X 4051 sets in `src/layout/wrap.rs`. `tests/` cannot import
/// them because the package builds a binary and no library.
fn is_no_break_start(c: char) -> bool {
    matches!(c,
        '）' | '〕' | '］' | '｝' | '〉' | '》' | '」' | '』' | '】' | '｠' | '〙' | '〗' | '»'
        | '‐' | '〜' | '！' | '？' | '‼' | '⁇' | '⁈' | '⁉' | '・' | '：' | '；'
        | '。' | '．' | '、' | '，' | 'ゝ' | 'ゞ' | 'ヽ' | 'ヾ' | '々' | '〻' | 'ー'
        | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ' | 'ゕ' | 'ゖ'
        | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヵ' | 'ヶ'
        | '\u{31F0}'..='\u{31FF}'
        | '｡' | '､' | '｣' | '\u{FF67}'..='\u{FF6F}' | 'ｰ'
    )
}

fn is_no_break_end(c: char) -> bool {
    matches!(
        c,
        '（' | '〔'
            | '［'
            | '｛'
            | '〈'
            | '《'
            | '「'
            | '『'
            | '【'
            | '｟'
            | '〘'
            | '〖'
            | '«'
            | '｢'
    )
}

/// A line may only exceed the requested width when kinsoku left nowhere legal
/// to break (追い込み). In that case the position where the line first runs over
/// budget must itself be a forbidden break point.
fn kinsoku_forbids_breaking_at_the_width(line: &str, width: usize) -> bool {
    use unicode_width::UnicodeWidthChar;
    let mut used = 0usize;
    let mut previous = None;
    for c in line.chars() {
        let next = used + UnicodeWidthChar::width(c).unwrap_or(0);
        if next > width {
            return is_no_break_start(c) || previous.is_some_and(is_no_break_end);
        }
        used = next;
        previous = Some(c);
    }
    false
}

#[test]
fn every_rendered_line_fits_the_requested_width_unless_kinsoku_forbids_it() {
    use unicode_width::UnicodeWidthStr;
    for fixture in [
        "tests/fixtures/gfm/showcase.md",
        "tests/fixtures/gfm/tables.md",
        "tests/fixtures/gfm/nested.md",
        // Reaches the vertical table fallback at narrow widths.
        "tests/fixtures/gfm/wide-table.md",
        "tests/fixtures/japanese/showcase.md",
        "tests/fixtures/japanese/punctuation.md",
        "tests/fixtures/japanese/tables.md",
    ] {
        for width in [40usize, 80, 120] {
            for line in plain(fixture, &width.to_string()).lines() {
                // Code and diagram lines are deliberately not wrapped; the pager
                // scrolls them horizontally instead.
                if line.starts_with('│') && !line.ends_with('│') {
                    continue;
                }
                let measured = UnicodeWidthStr::width(line);
                assert!(
                    measured <= width || kinsoku_forbids_breaking_at_the_width(line, width),
                    "{fixture} at width {width}: {line:?} is {measured} columns \
                     and could have been broken within budget"
                );
            }
        }
    }
}

golden!(gfm_code_80, "tests/fixtures/gfm/code.md", "80");

/// Highlighting must classify tokens without changing a single character, so
/// the plain snapshot above and this one carry the same text.
#[test]
fn gfm_code_highlighted_80() {
    insta::assert_snapshot!(styled("tests/fixtures/gfm/code.md", "80"));
}
