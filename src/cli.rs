use std::io::IsTerminal;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, ValueEnum, parser::ValueSource};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Debug, Parser)]
#[command(
    name = "mdvu",
    version,
    about = "A fast terminal Markdown viewer for GFM and Azure DevOps Wiki Markdown",
    styles = STYLES
)]
pub struct Cli {
    /// Markdown file or "-" for stdin
    pub file: Option<String>,

    /// Force the interactive pager
    #[arg(short = 'p', long, conflicts_with = "no_pager")]
    pub pager: bool,

    /// Render to stdout without entering the alternate screen
    #[arg(long = "no-pager")]
    pub no_pager: bool,

    /// Override the rendering width
    #[arg(short = 'w', long, value_name = "COLUMNS", value_parser = clap::value_parser!(u16).range(1..))]
    pub width: Option<u16>,

    /// Open near the given 1-based source line
    #[arg(short = 'l', long, value_name = "LINE", value_parser = clap::value_parser!(u64).range(1..))]
    pub line: Option<u64>,

    /// Markdown flavor
    #[arg(long, value_enum, default_value_t = Flavor::AzureDevops)]
    pub flavor: Flavor,

    /// Mermaid rendering mode
    #[arg(long, value_enum, default_value_t = MermaidMode::Unicode, value_name = "MODE")]
    pub mermaid: MermaidMode,

    /// Theme
    #[arg(long, value_enum, default_value_t = Theme::Auto)]
    pub theme: Theme,

    /// ANSI color policy
    #[arg(long, value_enum, default_value_t = ColorWhen::Auto, value_name = "WHEN")]
    pub color: ColorWhen,

    /// Alias for --color never
    #[arg(long)]
    pub plain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Flavor {
    Gfm,
    #[value(name = "azure-devops")]
    AzureDevops,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum MermaidMode {
    #[default]
    Unicode,
    Ascii,
    Source,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorWhen {
    Auto,
    Always,
    Never,
}

/// Where the Markdown source comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    File(PathBuf),
    Stdin,
}

/// Which backend renders the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Pager,
    Stdout,
}

/// Whether the stdout backend emits ANSI escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Ansi,
    Plain,
}

/// Terminal facts that mode resolution depends on. Kept explicit so resolution
/// stays a pure function and remains testable without a TTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalContext {
    pub stdout_is_tty: bool,
    pub stdin_is_tty: bool,
    pub no_color_env: bool,
}

impl TerminalContext {
    pub fn detect() -> Self {
        Self {
            stdout_is_tty: std::io::stdout().is_terminal(),
            stdin_is_tty: std::io::stdin().is_terminal(),
            no_color_env: std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
        }
    }
}

impl Cli {
    /// Parse argv, rejecting the invalid combinations listed in the CLI contract.
    /// Usage errors exit with code 2 via clap.
    pub fn parse_checked() -> Self {
        let matches = Self::command().get_matches();
        Self::from_checked_matches(&matches).unwrap_or_else(|err| err.exit())
    }

    fn from_checked_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let cli = Self::from_arg_matches(matches)?;
        // `--plain` is defined as an alias for `--color never`, so pairing it with an
        // explicit `--color always` is contradictory. A defaulted `--color` is not.
        let color_from_cli = matches.value_source("color") == Some(ValueSource::CommandLine);
        if cli.plain && color_from_cli && cli.color == ColorWhen::Always {
            return Err(Self::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                "the argument '--plain' cannot be used with '--color always'",
            ));
        }
        Ok(cli)
    }

    pub fn input_source(
        &self,
        ctx: TerminalContext,
    ) -> Result<InputSource, crate::error::AppError> {
        match self.file.as_deref() {
            Some("-") => Ok(InputSource::Stdin),
            Some(path) => Ok(InputSource::File(PathBuf::from(path))),
            None if !ctx.stdin_is_tty => Ok(InputSource::Stdin),
            None => Err(crate::error::AppError::MissingInput),
        }
    }

    pub fn output_mode(&self, ctx: TerminalContext) -> OutputMode {
        if self.pager {
            OutputMode::Pager
        } else if self.no_pager || !ctx.stdout_is_tty {
            OutputMode::Stdout
        } else {
            OutputMode::Pager
        }
    }

    pub fn color_choice(&self, ctx: TerminalContext, mode: OutputMode) -> ColorChoice {
        if self.plain {
            return ColorChoice::Plain;
        }
        match self.color {
            ColorWhen::Always => ColorChoice::Ansi,
            ColorWhen::Never => ColorChoice::Plain,
            ColorWhen::Auto => {
                if ctx.no_color_env {
                    ColorChoice::Plain
                } else if mode == OutputMode::Pager || ctx.stdout_is_tty {
                    ColorChoice::Ansi
                } else {
                    ColorChoice::Plain
                }
            }
        }
    }

    pub fn start_line(&self) -> Option<usize> {
        self.line.map(|n| n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        let argv = std::iter::once("mdvu").chain(args.iter().copied());
        let matches = Cli::command().get_matches_from(argv);
        Cli::from_checked_matches(&matches).expect("expected valid arguments")
    }

    fn try_cli(args: &[&str]) -> Result<Cli, clap::Error> {
        let argv = std::iter::once("mdvu").chain(args.iter().copied());
        let matches = Cli::command().try_get_matches_from(argv)?;
        Cli::from_checked_matches(&matches)
    }

    const TTY: TerminalContext = TerminalContext {
        stdout_is_tty: true,
        stdin_is_tty: true,
        no_color_env: false,
    };
    const PIPED: TerminalContext = TerminalContext {
        stdout_is_tty: false,
        stdin_is_tty: false,
        no_color_env: false,
    };

    #[test]
    fn defaults_match_the_cli_contract() {
        let cli = cli(&["a.md"]);
        assert_eq!(cli.flavor, Flavor::AzureDevops);
        assert_eq!(cli.mermaid, MermaidMode::Unicode);
        assert_eq!(cli.theme, Theme::Auto);
        assert_eq!(cli.color, ColorWhen::Auto);
    }

    #[test]
    fn dash_selects_stdin() {
        assert_eq!(cli(&["-"]).input_source(TTY).unwrap(), InputSource::Stdin);
    }

    #[test]
    fn omitted_file_reads_piped_stdin() {
        assert_eq!(cli(&[]).input_source(PIPED).unwrap(), InputSource::Stdin);
    }

    #[test]
    fn omitted_file_on_a_tty_is_an_error() {
        assert!(cli(&[]).input_source(TTY).is_err());
    }

    #[test]
    fn mode_resolution_follows_priority_order() {
        assert_eq!(cli(&["a.md"]).output_mode(TTY), OutputMode::Pager);
        assert_eq!(cli(&["a.md"]).output_mode(PIPED), OutputMode::Stdout);
        assert_eq!(
            cli(&["--pager", "a.md"]).output_mode(PIPED),
            OutputMode::Pager
        );
        assert_eq!(
            cli(&["--no-pager", "a.md"]).output_mode(TTY),
            OutputMode::Stdout
        );
    }

    #[test]
    fn color_auto_respects_no_color_and_tty() {
        let ctx = TerminalContext {
            no_color_env: true,
            ..TTY
        };
        assert_eq!(
            cli(&["a.md"]).color_choice(ctx, OutputMode::Pager),
            ColorChoice::Plain
        );
        assert_eq!(
            cli(&["a.md"]).color_choice(PIPED, OutputMode::Stdout),
            ColorChoice::Plain
        );
        assert_eq!(
            cli(&["a.md"]).color_choice(TTY, OutputMode::Pager),
            ColorChoice::Ansi
        );
    }

    #[test]
    fn color_always_wins_over_a_pipe() {
        assert_eq!(
            cli(&["--color", "always", "a.md"]).color_choice(PIPED, OutputMode::Stdout),
            ColorChoice::Ansi
        );
    }

    #[test]
    fn plain_disables_ansi_everywhere() {
        assert_eq!(
            cli(&["--plain", "a.md"]).color_choice(TTY, OutputMode::Pager),
            ColorChoice::Plain
        );
    }

    #[test]
    fn plain_alongside_a_defaulted_color_is_accepted() {
        assert!(try_cli(&["--plain", "a.md"]).is_ok());
        assert!(try_cli(&["--plain", "--color", "never", "a.md"]).is_ok());
    }

    #[test]
    fn rejected_combinations() {
        assert!(try_cli(&["--plain", "--color", "always", "a.md"]).is_err());
        assert!(try_cli(&["--pager", "--no-pager", "a.md"]).is_err());
        assert!(try_cli(&["--width", "0", "a.md"]).is_err());
        assert!(try_cli(&["--line", "0", "a.md"]).is_err());
    }
}
