//! Optional configuration file.
//!
//! The file only sets defaults for flags that already exist; nothing here adds
//! behaviour of its own. A value given on the command line always wins, so
//! `mdvu` behaves the same everywhere once a flag is passed explicitly.
//!
//! Values are parsed with clap's own `ValueEnum`, so the accepted spellings can
//! never drift from `--help`.

use std::path::PathBuf;

use clap::ValueEnum;
use serde::Deserialize;

use crate::cli::{ColorWhen, HighlightWhen, HyperlinkWhen, ImagesWhen};
use crate::diagram::MermaidMode;
use crate::flavor::Flavor;
use crate::layout::icons::IconSet;
use crate::layout::theme::ThemeChoice;

/// Overrides an absent flag would otherwise take from its built-in default.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    pub flavor: Option<Flavor>,
    pub mermaid: Option<MermaidMode>,
    pub theme: Option<ThemeChoice>,
    pub color: Option<ColorWhen>,
    pub hyperlinks: Option<HyperlinkWhen>,
    pub highlight: Option<HighlightWhen>,
    pub images: Option<ImagesWhen>,
    pub icons: Option<IconSet>,
    pub width: Option<u16>,
    pub watch: Option<bool>,
}

/// The file as written. Kept separate from `Config` so every value is validated
/// on the way out, with the same wording the CLI uses.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    flavor: Option<String>,
    mermaid: Option<String>,
    theme: Option<String>,
    color: Option<String>,
    hyperlinks: Option<String>,
    highlight: Option<String>,
    images: Option<String>,
    icons: Option<String>,
    width: Option<i64>,
    watch: Option<bool>,
}

/// Read the configuration file, if there is one.
///
/// `MDVU_CONFIG` overrides the search and, when set to an empty string, turns
/// the file off entirely. That is what the tests use so they never depend on
/// the machine they run on.
pub fn load() -> Result<Config, String> {
    let Some(path) = path() else {
        return Ok(Config::default());
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // A missing file is only an error when the reader named it themselves.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && !explicit() => {
            return Ok(Config::default());
        }
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    parse(&text).map_err(|err| format!("{}: {err}", path.display()))
}

fn explicit() -> bool {
    std::env::var_os("MDVU_CONFIG").is_some()
}

fn path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("MDVU_CONFIG") {
        return match value.is_empty() {
            true => None,
            false => Some(PathBuf::from(value)),
        };
    }
    // `~/.config` on every platform, so a dotfiles repo can carry one file.
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("mdvu").join("config.toml"))
}

pub fn parse(text: &str) -> Result<Config, String> {
    let raw: Raw = toml::from_str(text).map_err(|err| one_line(&err.to_string()))?;
    Ok(Config {
        flavor: value(raw.flavor.as_deref(), "flavor")?,
        mermaid: value(raw.mermaid.as_deref(), "mermaid")?,
        theme: value(raw.theme.as_deref(), "theme")?,
        color: value(raw.color.as_deref(), "color")?,
        hyperlinks: value(raw.hyperlinks.as_deref(), "hyperlinks")?,
        highlight: value(raw.highlight.as_deref(), "highlight")?,
        images: value(raw.images.as_deref(), "images")?,
        icons: value(raw.icons.as_deref(), "icons")?,
        width: width(raw.width)?,
        watch: raw.watch,
    })
}

fn value<T: ValueEnum>(raw: Option<&str>, key: &str) -> Result<Option<T>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    T::from_str(raw, true)
        .map(Some)
        .map_err(|_| format!("invalid value '{raw}' for '{key}': {}", possible::<T>()))
}

fn possible<T: ValueEnum>() -> String {
    let names: Vec<String> = T::value_variants()
        .iter()
        .filter_map(|v| v.to_possible_value().map(|p| p.get_name().to_string()))
        .collect();
    format!("possible values: {}", names.join(", "))
}

fn width(raw: Option<i64>) -> Result<Option<u16>, String> {
    match raw {
        None => Ok(None),
        Some(value) if (1..=i64::from(u16::MAX)).contains(&value) => Ok(Some(value as u16)),
        Some(value) => Err(format!(
            "invalid value '{value}' for 'width': expected 1..={}",
            u16::MAX
        )),
    }
}

/// Flatten a TOML error to one line, like every other usage error.
///
/// The excerpt it draws under the offending line is dropped; the position and
/// the reason are what the reader needs, and both are plain text.
fn one_line(message: &str) -> String {
    let parts: Vec<&str> = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_excerpt(line))
        .collect();
    match parts.is_empty() {
        true => message.trim().to_string(),
        false => parts.join("; "),
    }
}

/// `  |`, `1 | width = ` and the `^^^` marker under it.
fn is_excerpt(line: &str) -> bool {
    line.starts_with('|')
        || line.starts_with('^')
        || line.split_once('|').is_some_and(|(head, _)| {
            !head.is_empty() && head.trim().chars().all(|c| c.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_sets_nothing() {
        assert_eq!(parse("").expect("empty is valid"), Config::default());
    }

    #[test]
    fn every_key_maps_to_its_cli_spelling() {
        let config = parse(
            r#"
            flavor = "gfm"
            mermaid = "ascii"
            theme = "light"
            color = "always"
            hyperlinks = "never"
            highlight = "never"
            images = "kitty"
            icons = "nerd"
            width = 100
            "#,
        )
        .expect("valid configuration");
        assert_eq!(
            config,
            Config {
                flavor: Some(Flavor::Gfm),
                mermaid: Some(MermaidMode::Ascii),
                theme: Some(ThemeChoice::Light),
                color: Some(ColorWhen::Always),
                hyperlinks: Some(HyperlinkWhen::Never),
                highlight: Some(HighlightWhen::Never),
                images: Some(ImagesWhen::Kitty),
                icons: Some(IconSet::Nerd),
                width: Some(100),
                watch: None,
            }
        );
    }

    #[test]
    fn the_azure_flavor_keeps_its_hyphen() {
        assert_eq!(
            parse("flavor = \"azure-devops\"").expect("valid").flavor,
            Some(Flavor::AzureDevops)
        );
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        // A typo must not silently do nothing.
        let err = parse("flavour = \"gfm\"").expect_err("unknown key");
        assert!(err.contains("flavour"), "{err}");
    }

    #[test]
    fn an_invalid_value_lists_the_alternatives() {
        let err = parse("mermaid = \"svg\"").expect_err("invalid value");
        assert!(err.contains("svg"), "{err}");
        assert!(err.contains("unicode"), "{err}");
    }

    #[test]
    fn an_unknown_glyph_set_is_rejected() {
        let err = parse("icons = \"emoji\"").expect_err("invalid value");
        assert!(err.contains("emoji"), "{err}");
        assert!(err.contains("nerd"), "{err}");
    }

    #[test]
    fn width_follows_the_same_bounds_as_the_flag() {
        assert_eq!(parse("width = 1").expect("valid").width, Some(1));
        assert!(parse("width = 0").is_err());
        assert!(parse("width = -5").is_err());
        assert!(parse("width = 70000").is_err());
    }

    #[test]
    fn a_syntax_error_stays_on_one_line() {
        let err = parse("width = ").expect_err("malformed toml");
        assert!(!err.contains('\n'), "{err}");
    }
}
