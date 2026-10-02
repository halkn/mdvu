mod cli;
mod config;
mod diagnostic;
mod diagram;
mod document;
mod error;
mod flavor;
mod image;
mod input;
mod layout;
mod markdown;
mod output;
mod pager;
mod source;

use cli::{Cli, Settings, TerminalContext};
use error::{AppError, Result};
use layout::LayoutOptions;
use layout::inline::InlineContext;
use layout::theme::{Theme, Variant};

/// Used when the terminal size is unavailable, such as when stdout is a pipe.
const DEFAULT_WIDTH: usize = 80;

fn main() {
    let cli = Cli::parse_checked();
    let ctx = TerminalContext::detect();
    if let Err(err) = cli.resolve(ctx).and_then(|settings| run(&settings, ctx)) {
        if err.is_broken_pipe() {
            return;
        }
        eprintln!("mdvu: {err}");
        std::process::exit(AppError::EXIT_FAILURE);
    }
}

fn run(settings: &Settings, ctx: TerminalContext) -> Result<()> {
    let loaded = input::load(&settings.input)?;
    let theme = Theme::new(Variant::resolve(settings.theme));
    let document = document::build(loaded.text, settings.flavor, settings.mermaid);
    let inline = InlineContext {
        mermaid: settings.mermaid,
        // The root depends on the path alone, so a reload under `--watch` keeps
        // the boundary the document was opened with.
        content_root: loaded.base_dir.as_deref().and_then(image::content_root),
        base_dir: loaded.base_dir,
        highlight: settings.highlight,
        images: settings.images.map(image::ImageSupport::detect),
        icons: settings.icons,
    };
    match settings.mode {
        cli::OutputMode::Pager => pager::run(pager::PagerInput {
            document,
            inline,
            theme,
            title: loaded.display_name,
            flavor: flavor_label(settings.flavor),
            width_override: settings.width.map(usize::from),
            start_line: settings.line,
            watched: watched(settings),
        }),
        cli::OutputMode::Stdout => {
            let options = LayoutOptions::new(resolve_width(settings, ctx));
            let rendered = layout::layout_document(&document, options, &inline);
            let mut out = std::io::stdout().lock();
            output::write_document(
                &mut out,
                &rendered,
                settings.color,
                &theme,
                settings.hyperlinks,
            )
        }
    }
}

/// The file to follow under `--watch`. Stdin has no path, so it is never
/// watched; the CLI already rejects that combination.
fn watched(settings: &Settings) -> Option<pager::Watched> {
    match &settings.input {
        input::InputSource::File(path) if settings.watch => Some(pager::Watched {
            path: path.clone(),
            flavor: settings.flavor,
            mermaid: settings.mermaid,
        }),
        _ => None,
    }
}

fn flavor_label(flavor: flavor::Flavor) -> String {
    clap::ValueEnum::to_possible_value(&flavor)
        .expect("every flavor has a name")
        .get_name()
        .to_string()
}

fn resolve_width(settings: &Settings, ctx: TerminalContext) -> usize {
    if let Some(width) = settings.width {
        return width as usize;
    }
    if ctx.stdout_is_tty
        && let Ok((columns, _)) = crossterm::terminal::size()
        && columns > 0
    {
        return columns as usize;
    }
    DEFAULT_WIDTH
}
