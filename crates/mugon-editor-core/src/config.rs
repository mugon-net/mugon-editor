//! User configuration stored in the `[editor]` table of `~/.mugon/config.toml` (the file mugon-cli uses too).
//!
//! Currently only holds the library folder. The library is resolved in this order: explicit `--library`,
//! the `MUGON_EDITOR_LIBRARY` environment variable (both handled by clap), then this file, then (CLI only, when
//! interactive) a prompt on first start.

use anyhow::{bail, Context, Result};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    pub library: Option<PathBuf>,
}

/// `~/.mugon` (`%USERPROFILE%.mugon` on Windows).
pub fn config_dir() -> Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).context("cannot determine the user directory")?;
    Ok(PathBuf::from(home).join(".mugon"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn read_table(path: &Path) -> Result<toml::Table> {
    if !path.is_file() {
        return Ok(toml::Table::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Config> {
        let library = read_table(path)?
            .get("editor")
            .and_then(|editor| editor.get("library"))
            .and_then(|library| library.as_str())
            .map(PathBuf::from);
        Ok(Config { library })
    }

    /// Writes the `[editor]` table and keeps every other key of the file (mugon-cli's credentials, ...).
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut table = read_table(path)?;
        let mut editor = match table.remove("editor") {
            Some(toml::Value::Table(editor)) => editor,
            _ => toml::Table::new(),
        };
        match &self.library {
            Some(library) => {
                editor.insert("library".into(), toml::Value::String(library.to_string_lossy().into_owned()));
            }
            None => {
                editor.remove("library");
            }
        }
        if !editor.is_empty() {
            table.insert("editor".into(), toml::Value::Table(editor));
        }
        std::fs::write(path, toml::to_string_pretty(&table)?).with_context(|| format!("writing {}", path.display()))
    }

    pub fn load() -> Result<Config> {
        Self::load_from(&config_path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path()?)
    }
}

/// Library folder from the explicit value (flag/env) or the config file. `Ok(None)` if neither is set.
pub fn configured_library(explicit: Option<PathBuf>) -> Result<Option<PathBuf>> {
    if explicit.is_some() {
        return Ok(explicit.map(strip_verbatim));
    }
    Ok(Config::load()?.library.map(strip_verbatim))
}

/// Removes the Windows verbatim prefix (`\\?\`) that `canonicalize` adds. Verbatim paths are not normalised by
/// Windows, so tools that append `/name` (FBX2glTF) fail to open files below them.
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Like [`configured_library`], but asks on the terminal (and saves the answer) when nothing is configured.
pub fn resolve_library_interactive(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = configured_library(explicit)? {
        return Ok(p);
    }
    let path = config_path()?;
    if !std::io::stdin().is_terminal() {
        bail!("no library configured: pass --library, set MUGON_EDITOR_LIBRARY, or run `mugon-editor` interactively once to set it up ({})", path.display());
    }
    println!("Mugon Editor needs a folder for its asset library (imported models, textures, thumbnails; it gets large).");
    print!("Library folder: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let line = line.trim().trim_matches('"');
    if line.is_empty() {
        bail!("no folder given");
    }
    let library = PathBuf::from(line);
    std::fs::create_dir_all(&library).with_context(|| format!("creating {}", library.display()))?;
    let library = std::fs::canonicalize(&library).unwrap_or(library);
    let library = strip_verbatim(library);
    Config { library: Some(library.clone()) }.save()?;
    println!("Saved to {}", path.display());
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\E:\lib")), PathBuf::from(r"E:\lib"));
        assert_eq!(strip_verbatim(PathBuf::from(r"E:\lib")), PathBuf::from(r"E:\lib"));
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\UNC\srv\share")), PathBuf::from(r"\\?\UNC\srv\share"));
    }

    #[test]
    fn roundtrip_keeps_foreign_keys() {
        let path = std::env::temp_dir().join(format!("mugon-editor-config-test-{}/config.toml", std::process::id()));
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "mugon-net-api-url-override = \"http://x\"
[credentials]
game = \"key\"
").unwrap();
        let config = Config { library: Some(PathBuf::from("D:/lib")) };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("mugon-net-api-url-override") && text.contains("game = \"key\""), "{text}");
    }
}
