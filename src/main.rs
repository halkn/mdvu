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

use cli::{Cli, TerminalContext};
use error::{AppError, Result};
use layout::LayoutOptions;
use layout::inline::InlineContext;
use layout::theme::{Theme, Variant};

/// Used when the terminal size is unavailable, such as when stdout is a pipe.
const DEFAULT_WIDTH: usize = 80;

fn main() {
    let cli = Cli::parse_checked();
    if let Err(err) = run(&cli) {
        if err.is_broken_pipe() {
            return;
        }
        eprintln!("mdvu: {err}");
        std::process::exit(AppError::EXIT_FAILURE);
    }
}

fn run(cli: &Cli) -> Result<()> {
    let ctx = TerminalContext::detect();
    let source = cli.input_source(ctx)?;
    let loaded = input::load(&source)?;

    let mode = cli.output_mode(ctx);
    let color = cli.color_choice(ctx, mode);
    let theme = Theme::new(Variant::resolve(cli.theme));

    let images = cli.images(ctx, color);
    let mermaid = cli.mermaid_mode(images);

    let document = document::build(loaded.text, cli.flavor, mermaid);
    let inline = InlineContext {
        mermaid,
        // The root depends on the path alone, so a reload under `--watch` keeps
        // the boundary the document was opened with.
        content_root: loaded.base_dir.as_deref().and_then(image::content_root),
        base_dir: loaded.base_dir,
        highlight: cli.highlight(color),
        images: images.map(image::ImageSupport::detect),
        icons: cli.icons,
    };
    match mode {
        cli::OutputMode::Pager => pager::run(pager::PagerInput {
            document,
            inline,
            theme,
            title: loaded.display_name,
            flavor: flavor_label(cli.flavor),
            width_override: cli.width.map(usize::from),
            start_line: cli.start_line(),
            watched: watched(cli, &source, mermaid),
        }),
        cli::OutputMode::Stdout => {
            let options = LayoutOptions::new(resolve_width(cli, ctx));
            let rendered = layout::layout_document(&document, options, &inline);
            let mut out = std::io::stdout().lock();
            output::write_document(
                &mut out,
                &rendered,
                color,
                &theme,
                cli.hyperlinks(ctx, color),
            )
        }
    }
}

/// The file to follow under `--watch`. Stdin has no path, so it is never
/// watched; the CLI already rejects that combination.
fn watched(
    cli: &Cli,
    source: &input::InputSource,
    mermaid: diagram::MermaidMode,
) -> Option<pager::Watched> {
    match source {
        input::InputSource::File(path) if cli.watch => Some(pager::Watched {
            path: path.clone(),
            flavor: cli.flavor,
            mermaid,
        }),
        _ => None,
    }
}

fn flavor_label(flavor: flavor::Flavor) -> &'static str {
    match flavor {
        flavor::Flavor::Gfm => "gfm",
        flavor::Flavor::AzureDevops => "azure-devops",
    }
}

fn resolve_width(cli: &Cli, ctx: TerminalContext) -> usize {
    if let Some(width) = cli.width {
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
