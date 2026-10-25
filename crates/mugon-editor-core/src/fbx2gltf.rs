//! Locating FBX2glTF, and downloading the pinned upstream release into `~/.mugon/tools` when it is missing.
//!
//! The binary contains Autodesk's proprietary FBX SDK, so it is never bundled with the editor: it is fetched from
//! the upstream release page on first use and checked against a pinned SHA-256.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const VERSION: &str = "0.9.7";
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const LICENSE_NOTICE: &str = "FBX2glTF (BSD-3-Clause, Meta Platforms) is a separate program that bundles Autodesk's proprietary FBX SDK. \
It is downloaded unchanged from its upstream release page and not distributed by Mugon.";

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub file_name: &'static str,
}

/// The pinned upstream release asset for this platform, if upstream publishes one.
pub fn pinned_asset() -> Option<Asset> {
    let (asset, sha256, file_name) = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        ("FBX2glTF-windows-x64.exe", "8d90fb5e0a8d186a3d9a7ff8c75eaee541c3975ce4df0d80351f20092ae0877f", "FBX2glTF.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        ("FBX2glTF-linux-x64", "b38210352fdd29d50ba53a6514c3ff05ccbb5d1e0a6442db7a49834bf34a3145", "FBX2glTF")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        ("FBX2glTF-darwin-x64", "f82383ae4185c39f991b479b04ecce104f02e70c12a035ed31fc469e6f74a3fd", "FBX2glTF")
    } else {
        return None;
    };
    Some(Asset {
        url: format!("https://github.com/facebookincubator/FBX2glTF/releases/download/v{VERSION}/{asset}"),
        sha256: sha256.to_string(),
        file_name,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "FBX2glTF.exe"
    } else {
        "FBX2glTF"
    }
}

/// `~/.mugon/tools/fbx2gltf/<version>`.
pub fn tools_dir() -> Result<PathBuf> {
    Ok(crate::config::config_dir()?.join("tools").join("fbx2gltf").join(VERSION))
}

fn find_on_path(path_var: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path_var = path_var?;
    std::env::split_paths(&path_var).map(|dir| dir.join(executable_name())).find(|candidate| candidate.is_file())
}

fn fetch(url: &str) -> Result<Vec<u8>> {
    let mut response = ureq::get(url).call().with_context(|| format!("downloading {url}"))?;
    response.body_mut().with_config().limit(MAX_DOWNLOAD_BYTES).read_to_vec().with_context(|| format!("reading {url}"))
}

/// Writes `bytes` as `dir/<asset.file_name>` after checking the hash; nothing is written on a mismatch.
pub fn install_bytes(asset: &Asset, bytes: &[u8], dir: &Path) -> Result<PathBuf> {
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(&asset.sha256) {
        bail!("checksum mismatch for {}: expected {}, got {actual}", asset.url, asset.sha256);
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let target = dir.join(asset.file_name);
    let partial = dir.join(format!("{}.partial", asset.file_name));
    std::fs::write(&partial, bytes).with_context(|| format!("writing {}", partial.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&partial, &target).with_context(|| format!("installing {}", target.display()))?;
    Ok(target)
}

/// Explicit path, `PATH`, the tools folder, then a download of the pinned release.
pub fn resolve(explicit: Option<PathBuf>) -> Result<PathBuf> {
    resolve_with(explicit, std::env::var_os("PATH"), tools_dir()?, pinned_asset(), &fetch)
}

pub fn resolve_with(
    explicit: Option<PathBuf>,
    path_var: Option<std::ffi::OsString>,
    tools_dir: PathBuf,
    asset: Option<Asset>,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = find_on_path(path_var) {
        return Ok(path);
    }
    let installed = tools_dir.join(executable_name());
    if installed.is_file() {
        return Ok(installed);
    }
    let Some(asset) = asset else {
        bail!("FBX2glTF {VERSION} is not published for this platform; install it yourself and pass --fbx2gltf or set MUGON_EDITOR_FBX2GLTF");
    };
    eprintln!("{LICENSE_NOTICE}\nDownloading {} ...", asset.url);
    let bytes = fetch(&asset.url)?;
    let path = install_bytes(&asset, &bytes, &tools_dir)?;
    eprintln!("Installed FBX2glTF {VERSION} to {}", path.display());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset_for(bytes: &[u8]) -> Asset {
        Asset { url: "https://example.invalid/FBX2glTF".into(), sha256: sha256_hex(bytes), file_name: executable_name() }
    }

    fn no_fetch(_: &str) -> Result<Vec<u8>> {
        bail!("unexpected download")
    }

    #[test]
    fn explicit_path_wins() {
        let found = resolve_with(Some("custom".into()), None, PathBuf::from("unused"), None, &no_fetch).unwrap();
        assert_eq!(found, PathBuf::from("custom"));
    }

    #[test]
    fn path_comes_before_tools_dir() {
        let dir = tempfile::tempdir().unwrap();
        let on_path = dir.path().join("bin");
        std::fs::create_dir_all(&on_path).unwrap();
        std::fs::write(on_path.join(executable_name()), b"x").unwrap();
        let tools = dir.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join(executable_name()), b"y").unwrap();
        let path_var = std::env::join_paths([&on_path]).unwrap();
        let found = resolve_with(None, Some(path_var), tools, None, &no_fetch).unwrap();
        assert_eq!(found, on_path.join(executable_name()));
    }

    #[test]
    fn tools_dir_is_used_before_downloading() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(executable_name()), b"y").unwrap();
        let found = resolve_with(None, None, dir.path().to_path_buf(), None, &no_fetch).unwrap();
        assert_eq!(found, dir.path().join(executable_name()));
    }

    #[test]
    fn downloads_verified_binary_once() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"binary".to_vec();
        let asset = asset_for(&bytes);
        let fetch = |_: &str| Ok(bytes.clone());
        let found = resolve_with(None, None, dir.path().to_path_buf(), Some(asset.clone()), &fetch).unwrap();
        assert_eq!(std::fs::read(&found).unwrap(), b"binary");
        let again = resolve_with(None, None, dir.path().to_path_buf(), Some(asset), &no_fetch).unwrap();
        assert_eq!(found, again);
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let asset = asset_for(b"expected");
        let fetch = |_: &str| Ok(b"tampered".to_vec());
        let error = resolve_with(None, None, dir.path().to_path_buf(), Some(asset), &fetch).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
        assert!(!dir.path().join(executable_name()).exists());
    }

    #[test]
    fn unsupported_platform_points_at_the_flag() {
        let dir = tempfile::tempdir().unwrap();
        let error = resolve_with(None, None, dir.path().to_path_buf(), None, &no_fetch).unwrap_err();
        assert!(error.to_string().contains("--fbx2gltf"));
    }
}
