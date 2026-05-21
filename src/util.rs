use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

// ═══════════════════════════════════════════════════════════════════════════════
// Download
// ═══════════════════════════════════════════════════════════════════════════════

/// Build an HTTP client, optionally using a proxy or disabling TLS verification.
pub fn build_http_client(proxy_url: Option<&str>, ssl_verify: bool) -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(format!("sdk/{}", env!("CARGO_PKG_VERSION")))
        .danger_accept_invalid_certs(!ssl_verify);
    if let Some(url) = proxy_url {
        builder = builder.proxy(reqwest::Proxy::all(url)?);
    }
    Ok(builder.build()?)
}

/// Download a URL to `dest` with a progress bar.
pub fn download_with_progress(
    url: &str,
    headers: &HashMap<String, String>,
    dest: &Path,
    proxy_url: Option<&str>,
    ssl_verify: bool,
) -> Result<()> {
    let client = build_http_client(proxy_url, ssl_verify)?;
    let mut req = client.get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let mut response = req.send().with_context(|| format!("downloading {}", url))?;

    if !response.status().is_success() {
        bail!("HTTP {} while downloading {}", response.status(), url);
    }

    let total = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.set_message(
        Path::new(url)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Downloading...".to_string()),
    );

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(dest)
        .with_context(|| format!("creating {}", dest.display()))?;

    let mut buf = [0u8; 65536];
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        pb.inc(n as u64);
    }
    pb.finish_with_message("Downloaded");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extraction
// ═══════════════════════════════════════════════════════════════════════════════

/// Decompress `src` into `dest_dir`.  Supports:
/// - `.tar.gz` / `.tgz`
/// - `.tar.bz2` / `.tbz2`
/// - `.tar.xz` / `.tar.lzma`
/// - `.tar`
/// - `.zip`
/// - Bare file (copy to dest_dir)
pub fn extract(src: &str, dest_dir: &str) -> Result<()> {
    let src_path = Path::new(src);
    let dest_path = Path::new(dest_dir);
    std::fs::create_dir_all(dest_path)?;

    let name = src_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(src_path, dest_path)?;
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        extract_tar_bz2(src_path, dest_path)?;
    } else if name.ends_with(".tar.xz") || name.ends_with(".tar.lzma") {
        extract_tar_xz(src_path, dest_path)?;
    } else if name.ends_with(".tar") {
        extract_tar_plain(src_path, dest_path)?;
    } else if name.ends_with(".zip") {
        extract_zip(src_path, dest_path)?;
    } else {
        // Unknown format – try tar.gz first, then zip, then copy
        if extract_tar_gz(src_path, dest_path).is_ok() {
        } else if extract_zip(src_path, dest_path).is_ok() {
        } else {
            let file_name = src_path.file_name().unwrap_or_default();
            std::fs::copy(src_path, dest_path.join(file_name))?;
            return Ok(());
        }
    }

    // Strip single top-level directory (like tar --strip-components=1).
    // Most SDK archives wrap everything in one subdir (e.g. node-v22-win-x64/).
    strip_toplevel_dir(dest_path)
}

/// If `dest` contains exactly one subdirectory and nothing else, hoist its
/// contents up one level and remove the empty wrapper.
fn strip_toplevel_dir(dest: &Path) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(dest)
        .context("read dest after extraction")?
        .flatten()
        .collect();

    if entries.len() != 1 || !entries[0].path().is_dir() {
        return Ok(()); // Nothing to strip
    }

    let subdir = entries[0].path();
    // Rename subdir to a temp name inside dest to avoid conflicts
    let tmp = dest.join(".sdk-extract-tmp");
    std::fs::rename(&subdir, &tmp).context("rename to tmp")?;

    for child in std::fs::read_dir(&tmp).context("read tmp")?.flatten() {
        let dst = dest.join(child.file_name());
        std::fs::rename(child.path(), &dst).context("hoist child")?;
    }

    std::fs::remove_dir(&tmp).context("remove empty tmp")?;
    Ok(())
}

fn extract_tar_gz(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let gz   = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.set_overwrite(true);
    archive.unpack(dest).with_context(|| format!("extracting tar.gz {}", src.display()))
}

fn extract_tar_bz2(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let bz2  = flate2::read::DeflateDecoder::new(file);
    let mut archive = tar::Archive::new(bz2);
    archive.set_overwrite(true);
    archive.unpack(dest).with_context(|| format!("extracting tar.bz2 {}", src.display()))
}

fn extract_tar_xz(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let xz   = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(xz);
    archive.set_overwrite(true);
    archive.unpack(dest).with_context(|| format!("extracting tar.xz {}", src.display()))
}

fn extract_tar_plain(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let mut archive = tar::Archive::new(file);
    archive.set_overwrite(true);
    archive.unpack(dest).with_context(|| format!("extracting tar {}", src.display()))
}

fn extract_zip(src: &Path, dest: &Path) -> Result<()> {
    let file    = std::fs::File::open(src)?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("opening zip {}", src.display()))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let out_path  = dest.join(entry.name());
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            io::copy(&mut entry, &mut out)?;
            // Preserve executable bit on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode))?;
                }
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Filesystem helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Move (or copy+delete) a path from `src` to `dst`.
pub fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Try atomic rename first
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    // Cross-device: copy then delete
    copy_recursive(src, dst)?;
    if src.is_dir() {
        std::fs::remove_dir_all(src)?;
    } else {
        std::fs::remove_file(src)?;
    }
    Ok(())
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Ensure `.sdk/` is listed in the project's `.gitignore`.
pub fn ensure_gitignore(project_dir: &Path) -> Result<bool> {
    let gitignore_path = project_dir.join(".gitignore");
    let entry = ".sdk/\n";

    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.contains(".sdk") {
            return Ok(false);
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&gitignore_path)?;
        f.write_all(entry.as_bytes())?;
    } else {
        std::fs::write(&gitignore_path, entry)?;
    }
    Ok(true)
}
