use assert_cmd::Command;
use predicates::prelude::*;

fn mdvu() -> Command {
    let mut command = Command::cargo_bin("mdvu").expect("binary should be built");
    // Ignore whatever configuration the developer has; tests that need one
    // point `MDVU_CONFIG` at a temporary file of their own.
    command.env("MDVU_CONFIG", "");
    command
}

#[test]
fn renders_a_file_argument() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(&path, "# Hello\n").unwrap();

    mdvu()
        .args(["--paging", "never"])
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("# Hello"));
}

#[test]
fn dash_reads_stdin() {
    mdvu()
        .args(["--paging", "never", "-"])
        .write_stdin("# From stdin\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("# From stdin"));
}

#[test]
fn omitted_file_reads_piped_stdin() {
    mdvu()
        .args(["--paging", "never"])
        .write_stdin("# Piped\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("# Piped"));
}

#[test]
fn a_config_file_supplies_defaults_that_flags_override() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "width = 30\n").unwrap();
    let source = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do\n";

    let widest = |args: &[&str]| -> usize {
        let out = mdvu()
            .env("MDVU_CONFIG", &config)
            .args(args)
            .write_stdin(source)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::len)
            .max()
            .unwrap_or(0)
    };

    assert!(
        widest(&["--paging", "never", "-"]) <= 30,
        "config width applies"
    );
    // An explicit flag wins over the file.
    assert!(widest(&["--paging", "never", "--width", "60", "-"]) > 30);
}

#[test]
fn an_invalid_config_file_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "mermaid = \"svg\"\n").unwrap();

    mdvu()
        .env("MDVU_CONFIG", &config)
        .args(["--paging", "never", "-"])
        .write_stdin("# Hi\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("mermaid"));
}

#[test]
fn an_empty_mdvu_config_disables_the_file() {
    mdvu()
        .env("MDVU_CONFIG", "")
        .args(["--paging", "never", "-"])
        .write_stdin("# Hi\n")
        .assert()
        .success();
}

#[test]
fn hyperlinks_always_emits_osc_8_for_http_links() {
    mdvu()
        .args([
            "--paging",
            "never",
            "--color",
            "always",
            "--hyperlinks",
            "always",
            "-",
        ])
        .write_stdin("[docs](https://example.com/x) and [local](./other.md)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\x1b]8;;https://example.com/x\x1b\\",
        ))
        // A relative target is display-only and never becomes a hyperlink.
        .stdout(predicate::str::contains("]8;;./other.md").not());
}

#[test]
fn plain_output_stays_free_of_escapes_even_with_hyperlinks_always() {
    mdvu()
        .args([
            "--paging",
            "never",
            "--color",
            "never",
            "--hyperlinks",
            "always",
            "-",
        ])
        .write_stdin("[docs](https://example.com/x)\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b").not());
}

#[test]
fn watch_requires_a_file_and_the_pager() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(&path, "# Hi\n").unwrap();

    // stdin cannot be followed.
    mdvu()
        .args(["--watch", "-"])
        .write_stdin("# Hi\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--watch"));

    mdvu().arg("--watch").write_stdin("# Hi\n").assert().code(2);

    // Rendering once to stdout has nothing to re-render.
    mdvu()
        .args(["--watch", "--paging", "never"])
        .arg(&path)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--paging never"));
}

#[test]
fn directory_input_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();

    mdvu()
        .args(["--paging", "never"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("is a directory"));
}

#[test]
fn missing_file_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();

    mdvu()
        .args(["--paging", "never"])
        .arg(dir.path().join("absent.md"))
        .assert()
        .code(1);
}

#[test]
fn non_utf8_input_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("latin1.md");
    std::fs::write(&path, [b'#', b' ', 0xFF, b'\n']).unwrap();

    mdvu()
        .args(["--paging", "never"])
        .arg(&path)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not valid UTF-8"));
}

#[test]
fn removed_pager_and_plain_flags_are_usage_errors() {
    for flag in ["--pager", "-p", "--no-pager", "--plain"] {
        mdvu().args([flag, "-"]).write_stdin("x\n").assert().code(2);
    }
}

#[test]
fn unknown_paging_value_is_a_usage_error() {
    mdvu()
        .args(["--paging", "sometimes", "-"])
        .write_stdin("x\n")
        .assert()
        .code(2);
}

#[test]
fn zero_width_is_a_usage_error() {
    mdvu()
        .args(["--width", "0", "-"])
        .write_stdin("x\n")
        .assert()
        .code(2);
}

#[test]
fn zero_line_is_a_usage_error() {
    mdvu()
        .args(["--line", "0", "-"])
        .write_stdin("x\n")
        .assert()
        .code(2);
}

#[test]
fn unknown_flavor_is_a_usage_error() {
    mdvu()
        .args(["--flavor", "commonmark", "-"])
        .write_stdin("x\n")
        .assert()
        .code(2);
}

#[test]
fn help_and_version_succeed() {
    mdvu()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--flavor"));
    mdvu()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("mdvu"));
}

/// The fixture document holding a standalone image, with a real PNG beside it.
const WITH_IMAGE: &str = "tests/fixtures/gfm/showcase.md";

#[test]
fn a_named_protocol_draws_a_local_image() {
    let out = mdvu()
        .args([
            "--paging", "never", "--color", "always", "--images", "kitty", "--flavor", "gfm",
            WITH_IMAGE,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).expect("output is utf-8");
    assert!(
        out.contains("\x1b_Ga=T,f=100"),
        "expected a kitty placement"
    );
    // The picture replaces the placeholder rather than joining it.
    assert!(!out.contains("[image: architecture diagram]"));

    let iterm = mdvu()
        .args([
            "--paging", "never", "--color", "always", "--images", "iterm2", "--flavor", "gfm",
            WITH_IMAGE,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let iterm = String::from_utf8(iterm).expect("output is utf-8");
    assert!(iterm.contains("\x1b]1337;File=inline=1;"));
}

/// `auto` needs a terminal: a captured stdout, such as an `fzf --preview` pane,
/// would show the escape bytes rather than the picture.
#[test]
fn images_are_off_when_stdout_is_not_a_terminal() {
    mdvu()
        .args([
            "--paging", "never", "--color", "always", "--flavor", "gfm", WITH_IMAGE,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[image: architecture diagram]"));
}

/// Images are escape sequences, so the plain contract covers them too.
#[test]
fn plain_output_never_carries_an_image() {
    let out = mdvu()
        .args([
            "--paging", "never", "--color", "never", "--images", "kitty", "--flavor", "gfm",
            WITH_IMAGE,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).expect("output is utf-8");
    assert!(!out.contains('\x1b'));
    assert!(out.contains("[image: architecture diagram]"));
}

#[test]
fn an_image_outside_the_document_directory_keeps_its_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside.png");
    std::fs::copy("tests/fixtures/gfm/.attachments/architecture.png", &outside).unwrap();
    let doc = dir.path().join("doc").join("page.md");
    std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
    std::fs::write(&doc, "![a](../outside.png)\n").unwrap();

    let out = mdvu()
        .args([
            "--paging", "never", "--color", "always", "--images", "kitty",
        ])
        .arg(&doc)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).expect("output is utf-8");
    assert!(!out.contains("\x1b_G"));
    assert!(out.contains("[image: a]"));
}

/// A wiki keeps its attachments at the repository root, so a page in a
/// subdirectory reaches them with `../` or with a path from the root.
#[test]
fn a_page_reads_attachments_from_the_repository_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("wiki");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join(".attachments")).unwrap();
    std::fs::create_dir_all(root.join("design")).unwrap();
    std::fs::copy(
        "tests/fixtures/gfm/.attachments/architecture.png",
        root.join(".attachments/architecture.png"),
    )
    .unwrap();
    std::fs::copy(
        "tests/fixtures/gfm/.attachments/architecture.png",
        dir.path().join("outside.png"),
    )
    .unwrap();
    let doc = root.join("design/platform.md");
    std::fs::write(
        &doc,
        "![relative](../.attachments/architecture.png)\n\n\
         ![from the root](/.attachments/architecture.png)\n\n\
         ![escaping](../../outside.png)\n",
    )
    .unwrap();

    let out = mdvu()
        .args([
            "--paging", "never", "--color", "always", "--images", "kitty",
        ])
        .arg(&doc)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).expect("output is utf-8");
    assert_eq!(out.matches("\x1b_Ga=T,f=100").count(), 2);
    assert!(!out.contains("[image: relative]"));
    assert!(!out.contains("[image: from the root]"));
    // The root is the boundary: a path above it is still refused.
    assert!(out.contains("[image: escaping]"));
}

#[test]
fn a_document_on_stdin_has_no_directory_to_read_images_from() {
    let out = mdvu()
        .args([
            "--paging", "never", "--color", "always", "--images", "kitty", "-",
        ])
        .write_stdin("![a](tests/fixtures/gfm/.attachments/architecture.png)\n")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).expect("output is utf-8");
    assert!(!out.contains("\x1b_G"));
    assert!(out.contains("[image: a]"));
}
