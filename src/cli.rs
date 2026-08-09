use std::io::IsTerminal;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, ValueEnum, parser::ValueSource};

use crate::image::Protocol;
use crate::layout::icons::IconSet;

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

    /// OSC 8 terminal hyperlinks in stdout output
    #[arg(long, value_enum, default_value_t = HyperlinkWhen::Auto, value_name = "WHEN")]
    pub hyperlinks: HyperlinkWhen,

    /// Syntax highlighting for fenced code blocks
    #[arg(long, value_enum, default_value_t = HighlightWhen::Auto, value_name = "WHEN")]
    pub highlight: HighlightWhen,

    /// Draw local images with a terminal graphics protocol
    #[arg(long, value_enum, default_value_t = ImagesWhen::Auto, value_name = "WHEN")]
    pub images: ImagesWhen,

    /// Glyphs used for alerts, code fences and placeholders
    #[arg(long, value_enum, default_value_t = IconsSet::Unicode, value_name = "SET")]
    pub icons: IconsSet,

    /// Re-render the file when it changes on disk
    #[arg(long)]
    pub watch: bool,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HyperlinkWhen {
    Auto,
    Always,
    Never,
}

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

/// `nerd` is opt-in and never detected: whether a Nerd Font is installed is a
/// property of the terminal's font, and asking the terminal would mean writing
/// to the tty and waiting for an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum IconsSet {
    Unicode,
    Nerd,
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
        // `--plain` is defined as an alias for `--color never`, so pairing it with an
        // explicit `--color always` is contradictory. A defaulted `--color` is not.
        let color_from_cli = matches.value_source("color") == Some(ValueSource::CommandLine);
        if cli.plain && color_from_cli && cli.color == ColorWhen::Always {
            return Err(Self::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                "the argument '--plain' cannot be used with '--color always'",
            ));
        }
        if cli.watch {
            // There is nothing to follow when the document arrives on stdin,
            // and nothing to re-render when the pager is not running.
            if cli.no_pager {
                return Err(Self::command().error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "the argument '--watch' cannot be used with '--no-pager'",
                ));
            }
            if cli.file.as_deref().is_none_or(|file| file == "-") {
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
        let defaulted = |name: &str| matches.value_source(name) == Some(ValueSource::DefaultValue);

        if let Some(flavor) = config.flavor
            && defaulted("flavor")
        {
            self.flavor = flavor;
        }
        if let Some(mermaid) = config.mermaid
            && defaulted("mermaid")
        {
            self.mermaid = mermaid;
        }
        if let Some(theme) = config.theme
            && defaulted("theme")
        {
            self.theme = theme;
        }
        if let Some(color) = config.color
            && defaulted("color")
        {
            self.color = color;
        }
        if let Some(hyperlinks) = config.hyperlinks
            && defaulted("hyperlinks")
        {
            self.hyperlinks = hyperlinks;
        }
        if let Some(highlight) = config.highlight
            && defaulted("highlight")
        {
            self.highlight = highlight;
        }
        if let Some(images) = config.images
            && defaulted("images")
        {
            self.images = images;
        }
        if let Some(icons) = config.icons
            && defaulted("icons")
        {
            self.icons = icons;
        }
        // `--width` has no default, so an absent value means it is unset.
        if let Some(width) = config.width
            && self.width.is_none()
        {
            self.width = Some(width);
        }
        // Watching is only meaningful for a file shown in the pager. A
        // configured `watch = true` is skipped where it cannot apply rather
        // than turning an ordinary `mdvu -` into a usage error.
        if config.watch == Some(true)
            && !self.no_pager
            && self.file.as_deref().is_some_and(|file| file != "-")
        {
            self.watch = true;
        }
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

    /// Whether the stdout backend emits OSC 8 hyperlinks.
    ///
    /// Hyperlinks are escape sequences, so they follow the colour policy: a
    /// plain document stays free of every escape byte, including these.
    pub fn hyperlinks(&self, ctx: TerminalContext, color: ColorChoice) -> bool {
        if color == ColorChoice::Plain {
            return false;
        }
        match self.hyperlinks {
            HyperlinkWhen::Never => false,
            HyperlinkWhen::Always => true,
            // A capture such as `fzf --preview` cannot be detected, so `auto`
            // stays conservative and `--hyperlinks always` opts in.
            HyperlinkWhen::Auto => ctx.stdout_is_tty,
        }
    }

    /// Whether code blocks are split into syntax roles. Without ANSI every role
    /// would render as the same bytes, so highlighting is skipped entirely.
    pub fn highlight(&self, color: ColorChoice) -> bool {
        self.highlight == HighlightWhen::Auto && color == ColorChoice::Ansi
    }

    /// Which graphics protocol to draw images with, if any.
    ///
    /// Images are escape sequences, so they follow the colour policy for the
    /// same reason hyperlinks do: a plain document stays free of every escape
    /// byte. `auto` also requires a terminal, since a capture such as
    /// `fzf --preview` shows the bytes rather than the picture.
    pub fn images(&self, ctx: TerminalContext, color: ColorChoice) -> Option<Protocol> {
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

    /// Which glyph set the chrome is drawn with.
    ///
    /// Glyphs are ordinary characters rather than escape sequences, so unlike
    /// images and hyperlinks they are not tied to the colour policy: `--plain`
    /// still shows them.
    pub fn icons(&self) -> IconSet {
        match self.icons {
            IconsSet::Unicode => IconSet::Unicode,
            IconsSet::Nerd => IconSet::Nerd,
        }
    }

    pub fn start_line(&self) -> Option<usize> {
        self.line.map(|n| n as usize)
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
    fn configured_defaults_apply_when_a_flag_is_absent() {
        let config = Config {
            flavor: Some(Flavor::Gfm),
            mermaid: Some(MermaidMode::Ascii),
            theme: Some(Theme::Light),
            color: Some(ColorWhen::Never),
            hyperlinks: Some(HyperlinkWhen::Always),
            highlight: Some(HighlightWhen::Never),
            images: Some(ImagesWhen::Never),
            icons: Some(IconsSet::Nerd),
            width: Some(100),
            watch: None,
        };
        let cli = configured(&["a.md"], &config);
        assert_eq!(cli.flavor, Flavor::Gfm);
        assert_eq!(cli.mermaid, MermaidMode::Ascii);
        assert_eq!(cli.theme, Theme::Light);
        assert_eq!(cli.color, ColorWhen::Never);
        assert_eq!(cli.hyperlinks, HyperlinkWhen::Always);
        assert_eq!(cli.highlight, HighlightWhen::Never);
        assert_eq!(cli.images, ImagesWhen::Never);
        assert_eq!(cli.icons, IconsSet::Nerd);
        assert_eq!(cli.width, Some(100));
    }

    /// Glyphs are opt-in, and a reader who turned them on in the configuration
    /// can still ask for the plain set on a machine without the font.
    #[test]
    fn icons_default_to_unicode_and_are_chosen_explicitly() {
        assert_eq!(cli(&["a.md"]).icons(), IconSet::Unicode);
        assert_eq!(cli(&["--icons", "nerd", "a.md"]).icons(), IconSet::Nerd);
        let config = Config {
            icons: Some(IconsSet::Nerd),
            ..Config::default()
        };
        assert_eq!(
            configured(&["--icons", "unicode", "a.md"], &config).icons(),
            IconSet::Unicode
        );
    }

    /// Glyphs are characters, not escape sequences, so the colour policy has no
    /// say over them.
    #[test]
    fn icons_are_independent_of_the_colour_policy() {
        assert_eq!(
            cli(&["--plain", "--icons", "nerd", "a.md"]).icons(),
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
        assert!(
            !cli(&["--plain", "--hyperlinks", "always", "a.md"])
                .hyperlinks(TTY, ColorChoice::Plain)
        );
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
