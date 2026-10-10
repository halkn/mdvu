use assert_cmd::Command;

pub fn mdvu() -> Command {
    let mut command = Command::cargo_bin("mdvu").expect("binary should be built");
    command.env("MDVU_CONFIG", "");
    for variable in [
        "COLORFGBG",
        "NO_COLOR",
        "TERM",
        "TERM_PROGRAM",
        "KITTY_WINDOW_ID",
        "KONSOLE_VERSION",
        "TMUX",
    ] {
        command.env_remove(variable);
    }
    command
}
