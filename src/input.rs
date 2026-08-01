use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cli::InputSource;
use crate::error::{AppError, Result};

/// UTF-8 Markdown together with the provenance the renderer needs for the
/// status bar and for resolving relative link targets.
#[derive(Debug, Clone)]
pub struct LoadedInput {
    pub text: String,
    pub display_name: String,
    /// Directory used to display relative link targets. `None` for stdin, which
    /// has no meaningful base.
    pub base_dir: Option<PathBuf>,
}

pub fn load(source: &InputSource) -> Result<LoadedInput> {
    match source {
        InputSource::File(path) => load_file(path),
        InputSource::Stdin => load_stdin(),
    }
}

fn load_file(path: &Path) -> Result<LoadedInput> {
    let metadata = std::fs::metadata(path).map_err(|source| AppError::InputIo {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_dir() {
        return Err(AppError::InputIsDirectory {
            path: path.to_path_buf(),
        });
    }

    let bytes = std::fs::read(path).map_err(|source| AppError::InputIo {
        path: path.to_path_buf(),
        source,
    })?;
    let display_name = path.display().to_string();
    let text = decode(bytes, &display_name)?;

    Ok(LoadedInput {
        text,
        display_name,
        base_dir: path.parent().map(Path::to_path_buf),
    })
}

fn load_stdin() -> Result<LoadedInput> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .map_err(|source| AppError::StdinIo { source })?;
    let text = decode(bytes, "stdin")?;

    Ok(LoadedInput {
        text,
        display_name: "<stdin>".to_string(),
        base_dir: None,
    })
}

fn decode(bytes: Vec<u8>, origin: &str) -> Result<String> {
    // A leading BOM is stripped so it never reaches the layout engine as a
    // zero-width character on the first line.
    let bytes = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => rest.to_vec(),
        None => bytes,
    };
    String::from_utf8(bytes).map_err(|err| AppError::Decode {
        origin: origin.to_string(),
        offset: err.utf8_error().valid_up_to(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_utf8_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# 見出し\n").unwrap();

        let loaded = load(&InputSource::File(path.clone())).unwrap();
        assert_eq!(loaded.text, "# 見出し\n");
        assert_eq!(loaded.base_dir.as_deref(), path.parent());
    }

    #[test]
    fn rejects_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(&InputSource::File(dir.path().to_path_buf())).unwrap_err();
        assert!(matches!(err, AppError::InputIsDirectory { .. }));
    }

    #[test]
    fn rejects_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(&InputSource::File(dir.path().join("absent.md"))).unwrap_err();
        assert!(matches!(err, AppError::InputIo { .. }));
    }

    #[test]
    fn rejects_non_utf8_bytes() {
        let err = decode(vec![b'#', b' ', 0xFF], "stdin").unwrap_err();
        match err {
            AppError::Decode { offset, .. } => assert_eq!(offset, 2),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn strips_a_leading_bom() {
        let text = decode(b"\xEF\xBB\xBF# Title\n".to_vec(), "stdin").unwrap();
        assert_eq!(text, "# Title\n");
    }
}
