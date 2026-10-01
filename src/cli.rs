use std::io::IsTerminal;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, ValueEnum};

use crate::diagram::MermaidMode;
use crate::error::AppError;
use crate::flavor::Flavor;
use crate::image::Protocol;
use crate::input::InputSource;
use crate::layout::icons::IconSet;
use crate::layout::theme::ThemeChoice;
use crate::output::ColorChoice;

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

    /// When to open the interactive pager instead of writing to stdout
    #[arg(long, value_enum, default_value_t = When::Auto, value_name = "WHEN")]
    pub paging: When,

    /// Override the rendering width
    #[arg(short = 'w', long, value_name = "COLUMNS", value_parser = clap::value_parser!(u16).range(1..))]
    pub width: Option<u16>,

    /// Open near the given 1-based source line
    #[arg(short = 'l', long, value_name = "LINE", value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    pub line: Option<usize>,

    /// Markdown flavor
    #[arg(long, value_enum, default_value_t = Flavor::AzureDevops)]
    pub flavor: Flavor,

    /// Mermaid rendering mode
    #[arg(long, value_enum, default_value_t = MermaidMode::Unicode, value_name = "MODE")]
    pub mermaid: MermaidMode,

    /// Theme
    #[arg(long, value_enum, default_value_t = ThemeChoice::Auto)]
    pub theme: ThemeChoice,

    /// ANSI color policy
    #[arg(long, value_enum, default_value_t = When::Auto, value_name = "WHEN")]
    pub color: When,

    /// OSC 8 terminal hyperlinks in stdout output
    #[arg(long, value_enum, default_value_t = When::Auto, value_name = "WHEN")]
    pub hyperlinks: When,

    /// Syntax highlighting for fenced code blocks
    #[arg(long, value_enum, default_value_t = HighlightWhen::Auto, value_name = "WHEN")]
    pub highlight: HighlightWhen,

    /// Draw local images with a terminal graphics protocol
    #[arg(long, value_enum, default_value_t = ImagesWhen::Auto, value_name = "MODE")]
    pub images: ImagesWhen,

    /// Glyphs used for alerts, code fences and placeholders
    #[arg(long, value_enum, default_value_t = IconSet::Unicode, value_name = "SET")]
    pub icons: IconSet,

    /// Re-render the file when it changes on disk
    #[arg(long)]
    pub watch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum When {
    Auto,
    Always,
    Never,
}

/// `always` is missing because highlighting without ANSI would produce the
/// same bytes as no highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HighlightWhen {
    Auto,
    Never,
}

/// `auto` reads the environment; naming a protocol forces it, which is the only
/// way to get images inside a multiplexer or an unrecognised terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImagesWhen {
    Auto,
    Kitty,
    Iterm2,
    Never,
}

/// Which backend renders the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Pager,
    Stdout,
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

/// Every decision the flags and the terminal make together, taken once so the
/// rest of the program only reads the outcome.
#[derive(Debug)]
pub struct Settings {
    pub input: InputSource,
    pub mode: OutputMode,
    pub color: ColorChoice,
    pub hyperlinks: bool,
    pub images: Option<Protocol>,
    /// The mode actually drawn, after `image` fell back where images are off.
    pub mermaid: MermaidMode,
    pub highlight: bool,
    pub icons: IconSet,
    pub theme: ThemeChoice,
    pub flavor: Flavor,
    pub width: Option<u16>,
    pub line: Option<usize>,
    pub watch: bool,
}

impl Cli {
    /// Parse argv, rejecting the invalid combinations listed in the CLI contract.
    /// Usage errors exit with code 2 via clap.
    pub fn parse_checked() -> Self {
        let matches = Self::command().get_matches();
        let config = crate::config::load()
            .map_err(|message| {
                Self::command().error(clap::error::ErrorKind::ValueValidation, message)
            })
            .unwrap_or_else(|err| err.exit());
        Self::from_checked_matches(&matches, &config).unwrap_or_else(|err| err.exit())
    }

    fn from_checked_matches(
        matches: &ArgMatches,
        config: &crate::config::Config,
    ) -> Result<Self, clap::Error> {
        let mut cli = Self::from_arg_matches(matches)?;
        cli.apply(config, matches);
        if cli.watch {
            // There is nothing to follow when the document arrives on stdin,
            // and nothing to re-render when the pager is not running.
            if cli.paging == When::Never {
                return Err(Self::command().error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "the argument '--watch' cannot be used with '--paging never'",
                ));
            }
            if !cli.reads_a_file() {
                return Err(Self::command().error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "the argument '--watch' requires a FILE, not stdin",
                ));
            }
        }
        Ok(cli)
    }

    /// Take configured values for the flags that were left at their built-in
    /// default. An explicit flag is never overridden.
    fn apply(&mut self, config: &crate::config::Config, matches: &ArgMatches) {
        config.apply_defaults(self, matches);
        // `--width` has no default, so an absent value means it is unset.
        if let Some(width) = config.width
            && self.width.is_none()
        {
            self.width = Some(width);
        }
        // Watching is only meaningful for a file shown in the pager. A
        // configured `watch = true` is skipped where it cannot apply rather
        // than turning an ordinary `mdvu -` into a usage error.
        if config.watch == Some(true) && self.paging != When::Never && self.reads_a_file() {
            self.watch = true;
        }
    }

    fn reads_a_file(&self) -> bool {
        self.file.as_deref().is_some_and(|file| file != "-")
    }

    pub fn resolve(&self, ctx: TerminalContext) -> Result<Settings, AppError> {
        let mode = self.output_mode(ctx);
        let color = self.color_choice(ctx, mode);
        let images = self.images(ctx, color);
        Ok(Settings {
            input: self.input_source(ctx)?,
            mode,
            color,
            hyperlinks: self.hyperlinks(ctx, color),
            images,
            mermaid: self.mermaid_mode(images),
            highlight: self.highlight(color),
            icons: self.icons,
            theme: self.theme,
            flavor: self.flavor,
            width: self.width,
            line: self.line,
            watch: self.watch,
        })
    }

    fn input_source(&self, ctx: TerminalContext) -> Result<InputSource, AppError> {
        match self.file.as_deref() {
            Some("-") => Ok(InputSource::Stdin),
            Some(path) => Ok(InputSource::File(PathBuf::from(path))),
            None if !ctx.stdin_is_tty => Ok(InputSource::Stdin),
            None => Err(AppError::MissingInput),
        }
    }

    fn output_mode(&self, ctx: TerminalContext) -> OutputMode {
        match self.paging {
            When::Always => OutputMode::Pager,
            When::Never => OutputMode::Stdout,
            When::Auto if ctx.stdout_is_tty => OutputMode::Pager,
            When::Auto => OutputMode::Stdout,
        }
    }

    fn color_choice(&self, ctx: TerminalContext, mode: OutputMode) -> ColorChoice {
        match self.color {
            When::Always => ColorChoice::Ansi,
            When::Never => ColorChoice::Plain,
            When::Auto => {
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

    /// Whether the stdout backend emits OSC 8 hyperlinks.
    ///
    /// Hyperlinks are escape sequences, so they follow the colour policy: a
    /// plain document stays free of every escape byte, including these.
    fn hyperlinks(&self, ctx: TerminalContext, color: ColorChoice) -> bool {
        if color == ColorChoice::Plain {
            return false;
        }
        match self.hyperlinks {
            When::Never => false,
            When::Always => true,
            // A capture such as `fzf --preview` cannot be detected, so `auto`
            // stays conservative and `--hyperlinks always` opts in.
            When::Auto => ctx.stdout_is_tty,
        }
    }

    /// Whether code blocks are split into syntax roles. Without ANSI every role
    /// would render as the same bytes, so highlighting is skipped entirely.
    fn highlight(&self, color: ColorChoice) -> bool {
        self.highlight == HighlightWhen::Auto && color == ColorChoice::Ansi
    }

    /// Which graphics protocol to draw images with, if any.
    ///
    /// Images are escape sequences, so they follow the colour policy for the
    /// same reason hyperlinks do: a plain document stays free of every escape
    /// byte. `auto` also requires a terminal, since a capture such as
    /// `fzf --preview` shows the bytes rather than the picture.
    fn images(&self, ctx: TerminalContext, color: ColorChoice) -> Option<Protocol> {
        if color == ColorChoice::Plain {
            return None;
        }
        match self.images {
            ImagesWhen::Never => None,
            ImagesWhen::Kitty => Some(Protocol::Kitty),
            ImagesWhen::Iterm2 => Some(Protocol::Iterm2),
            ImagesWhen::Auto => ctx
                .stdout_is_tty
                .then(|| crate::image::protocol_from_env(&crate::image::Env::detect()))
                .flatten(),
        }
    }

    /// The Mermaid mode actually drawn. A picture needs a graphics protocol,
    /// so `image` becomes Unicode text wherever images are off.
    fn mermaid_mode(&self, images: Option<Protocol>) -> MermaidMode {
        match (self.mermaid, images) {
            (MermaidMode::Image, None) => MermaidMode::Unicode,
            (mode, _) => mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cli(args: &[&str]) -> Cli {
        configured(args, &Config::default())
    }

    /// Parse `args` as if `config` had been read from disk.
    fn configured(args: &[&str], config: &Config) -> Cli {
        let argv = std::iter::once("mdvu").chain(args.iter().copied());
        let matches = Cli::command().get_matches_from(argv);
        Cli::from_checked_matches(&matches, config).expect("expected valid arguments")
    }

    fn try_cli(args: &[&str]) -> Result<Cli, clap::Error> {
        let argv = std::iter::once("mdvu").chain(args.iter().copied());
        let matches = Cli::command().try_get_matches_from(argv)?;
        Cli::from_checked_matches(&matches, &Config::default())
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
        assert_eq!(cli.theme, ThemeChoice::Auto);
        assert_eq!(cli.color, When::Auto);
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
            cli(&["--paging", "always", "a.md"]).output_mode(PIPED),
            OutputMode::Pager
        );
        assert_eq!(
            cli(&["--paging", "never", "a.md"]).output_mode(TTY),
            OutputMode::Stdout
        );
        assert_eq!(
            cli(&["--paging", "auto", "a.md"]).output_mode(TTY),
            OutputMode::Pager
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
    fn color_never_disables_ansi_everywhere() {
        assert_eq!(
            cli(&["--color", "never", "a.md"]).color_choice(TTY, OutputMode::Pager),
            ColorChoice::Plain
        );
    }

    #[test]
    fn configured_defaults_apply_when_a_flag_is_absent() {
        let config = Config {
            flavor: Some(Flavor::Gfm),
            mermaid: Some(MermaidMode::Ascii),
            theme: Some(ThemeChoice::Light),
            color: Some(When::Never),
            hyperlinks: Some(When::Always),
            highlight: Some(HighlightWhen::Never),
            images: Some(ImagesWhen::Never),
            icons: Some(IconSet::Nerd),
            width: Some(100),
            watch: None,
        };
        let cli = configured(&["a.md"], &config);
        assert_eq!(cli.flavor, Flavor::Gfm);
        assert_eq!(cli.mermaid, MermaidMode::Ascii);
        assert_eq!(cli.theme, ThemeChoice::Light);
        assert_eq!(cli.color, When::Never);
        assert_eq!(cli.hyperlinks, When::Always);
        assert_eq!(cli.highlight, HighlightWhen::Never);
        assert_eq!(cli.images, ImagesWhen::Never);
        assert_eq!(cli.icons, IconSet::Nerd);
        assert_eq!(cli.width, Some(100));
    }

    /// Glyphs are opt-in, and a reader who turned them on in the configuration
    /// can still ask for the plain set on a machine without the font.
    #[test]
    fn icons_default_to_unicode_and_are_chosen_explicitly() {
        assert_eq!(cli(&["a.md"]).icons, IconSet::Unicode);
        assert_eq!(cli(&["--icons", "nerd", "a.md"]).icons, IconSet::Nerd);
        let config = Config {
            icons: Some(IconSet::Nerd),
            ..Config::default()
        };
        assert_eq!(
            configured(&["--icons", "unicode", "a.md"], &config).icons,
            IconSet::Unicode
        );
    }

    /// Glyphs are characters, not escape sequences, so the colour policy has no
    /// say over them.
    #[test]
    fn icons_are_independent_of_the_colour_policy() {
        assert_eq!(
            cli(&["--color", "never", "--icons", "nerd", "a.md"]).icons,
            IconSet::Nerd
        );
    }

    #[test]
    fn an_explicit_flag_beats_the_configuration() {
        let config = Config {
            flavor: Some(Flavor::Gfm),
            width: Some(100),
            watch: None,
            ..Config::default()
        };
        let cli = configured(
            &["--flavor", "azure-devops", "--width", "40", "a.md"],
            &config,
        );
        assert_eq!(cli.flavor, Flavor::AzureDevops);
        assert_eq!(cli.width, Some(40));
    }

    #[test]
    fn an_empty_configuration_leaves_the_built_in_defaults() {
        let cli = configured(&["a.md"], &Config::default());
        assert_eq!(cli.flavor, Flavor::AzureDevops);
        assert_eq!(cli.mermaid, MermaidMode::Unicode);
        assert_eq!(cli.width, None);
    }

    #[test]
    fn hyperlinks_follow_the_color_policy() {
        // A plain document must not contain any escape byte, hyperlinks included.
        assert!(!cli(&["--hyperlinks", "always", "a.md"]).hyperlinks(TTY, ColorChoice::Plain));
        assert!(cli(&["--hyperlinks", "always", "a.md"]).hyperlinks(PIPED, ColorChoice::Ansi));
        assert!(!cli(&["--hyperlinks", "never", "a.md"]).hyperlinks(TTY, ColorChoice::Ansi));
    }

    #[test]
    fn highlighting_needs_ansi_and_can_be_turned_off() {
        assert!(cli(&["a.md"]).highlight(ColorChoice::Ansi));
        assert!(!cli(&["a.md"]).highlight(ColorChoice::Plain));
        assert!(!cli(&["--highlight", "never", "a.md"]).highlight(ColorChoice::Ansi));
    }

    #[test]
    fn a_named_image_protocol_is_forced_and_never_escapes_a_plain_document() {
        assert_eq!(
            cli(&["--images", "kitty", "a.md"]).images(PIPED, ColorChoice::Ansi),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            cli(&["--images", "iterm2", "a.md"]).images(TTY, ColorChoice::Ansi),
            Some(Protocol::Iterm2)
        );
        assert_eq!(
            cli(&["--images", "kitty", "a.md"]).images(TTY, ColorChoice::Plain),
            None
        );
        assert_eq!(
            cli(&["--images", "never", "a.md"]).images(TTY, ColorChoice::Ansi),
            None
        );
    }

    #[test]
    fn mermaid_images_fall_back_to_text_without_a_protocol() {
        let image = cli(&["--mermaid", "image", "a.md"]);
        assert_eq!(
            image.mermaid_mode(Some(Protocol::Kitty)),
            MermaidMode::Image
        );
        assert_eq!(image.mermaid_mode(None), MermaidMode::Unicode);
        assert_eq!(
            cli(&["--mermaid", "ascii", "a.md"]).mermaid_mode(None),
            MermaidMode::Ascii
        );
    }

    /// A capture such as `fzf --preview` would show the escape bytes instead of
    /// a picture, so `auto` stays off without a terminal.
    #[test]
    fn images_auto_requires_a_terminal() {
        assert_eq!(cli(&["a.md"]).images(PIPED, ColorChoice::Ansi), None);
    }

    #[test]
    fn hyperlinks_auto_requires_a_terminal() {
        assert!(cli(&["a.md"]).hyperlinks(TTY, ColorChoice::Ansi));
        // `fzf --preview` captures stdout, so `auto` stays off there.
        assert!(!cli(&["a.md"]).hyperlinks(PIPED, ColorChoice::Ansi));
    }

    #[test]
    fn watch_needs_a_file_and_the_pager() {
        assert!(try_cli(&["--watch", "a.md"]).is_ok());
        assert!(try_cli(&["--watch", "--paging", "always", "a.md"]).is_ok());
        assert!(try_cli(&["--watch", "--paging", "never", "a.md"]).is_err());
        assert!(try_cli(&["--watch", "-"]).is_err());
    }

    /// A configured `watch = true` is dropped where it cannot apply instead of
    /// turning every `--paging never` run into a usage error.
    #[test]
    fn configured_watch_is_skipped_without_the_pager() {
        let config = Config {
            watch: Some(true),
            ..Config::default()
        };
        assert!(configured(&["a.md"], &config).watch);
        assert!(!configured(&["--paging", "never", "a.md"], &config).watch);
        assert!(!configured(&["-"], &config).watch);
    }

    #[test]
    fn rejected_combinations() {
        assert!(try_cli(&["--plain", "a.md"]).is_err());
        assert!(try_cli(&["--pager", "a.md"]).is_err());
        assert!(try_cli(&["--no-pager", "a.md"]).is_err());
        assert!(try_cli(&["--width", "0", "a.md"]).is_err());
        assert!(try_cli(&["--line", "0", "a.md"]).is_err());
    }
}
