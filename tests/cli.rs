use assert_cmd::Command;
use predicates::prelude::*;

fn mdvu() -> Command {
    Command::cargo_bin("mdvu").expect("binary should be built")
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
