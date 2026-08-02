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
        .arg("--no-pager")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("# Hello"));
}

#[test]
fn dash_reads_stdin() {
    mdvu()
        .args(["--no-pager", "-"])
        .write_stdin("# From stdin\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("# From stdin"));
}

#[test]
fn omitted_file_reads_piped_stdin() {
    mdvu()
        .arg("--no-pager")
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

    assert!(widest(&["--no-pager", "-"]) <= 30, "config width applies");
    // An explicit flag wins over the file.
    assert!(widest(&["--no-pager", "--width", "60", "-"]) > 30);
}

#[test]
fn an_invalid_config_file_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "mermaid = \"svg\"\n").unwrap();

    mdvu()
        .env("MDVU_CONFIG", &config)
        .args(["--no-pager", "-"])
        .write_stdin("# Hi\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("mermaid"));
}

#[test]
fn an_empty_mdvu_config_disables_the_file() {
    mdvu()
        .env("MDVU_CONFIG", "")
        .args(["--no-pager", "-"])
        .write_stdin("# Hi\n")
        .assert()
        .success();
}

#[test]
fn hyperlinks_always_emits_osc_8_for_http_links() {
    mdvu()
        .args([
            "--no-pager",
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
        .args(["--no-pager", "--plain", "--hyperlinks", "always", "-"])
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
        .args(["--watch", "--no-pager"])
        .arg(&path)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--no-pager"));
}

#[test]
fn directory_input_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();

    mdvu()
        .arg("--no-pager")
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("is a directory"));
}

#[test]
fn missing_file_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();

    mdvu()
        .arg("--no-pager")
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
        .arg("--no-pager")
        .arg(&path)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not valid UTF-8"));
}

#[test]
fn conflicting_pager_flags_are_a_usage_error() {
    mdvu()
        .args(["--pager", "--no-pager", "-"])
        .write_stdin("x\n")
        .assert()
        .code(2);
}

#[test]
fn plain_with_color_always_is_a_usage_error() {
    mdvu()
        .args(["--plain", "--color", "always", "-"])
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
