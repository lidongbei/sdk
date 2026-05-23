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
/// Supports resume: if `dest` already exists, sends a Range request to continue.
pub fn download_with_progress(
    url: &str,
    headers: &HashMap<String, String>,
    dest: &Path,
    proxy_url: Option<&str>,
    ssl_verify: bool,
) -> Result<()> {
    download_inner(url, headers, dest, proxy_url, ssl_verify, None, None)
}

/// Download a URL to `dest`, adding a per-file child bar to `multi` and
/// incrementing `overall` by one when the file completes.
/// Supports resume: if a partial `dest` exists, continues from where it stopped.
pub fn download_with_multi_progress(
    url: &str,
    headers: &HashMap<String, String>,
    dest: &Path,
    proxy_url: Option<&str>,
    ssl_verify: bool,
    multi: &indicatif::MultiProgress,
    overall: &indicatif::ProgressBar,
) -> Result<()> {
    download_inner(url, headers, dest, proxy_url, ssl_verify, Some(multi), Some(overall))
}

fn download_inner(
    url: &str,
    headers: &HashMap<String, String>,
    dest: &Path,
    proxy_url: Option<&str>,
    ssl_verify: bool,
    multi: Option<&indicatif::MultiProgress>,
    overall: Option<&indicatif::ProgressBar>,
) -> Result<()> {
    let client = build_http_client(proxy_url, ssl_verify)?;

    // Use a `.part` temp file while downloading; rename to `dest` on success.
    // If `.part` exists, resume from its current size.
    let part_path: PathBuf = {
        let mut p = dest.as_os_str().to_owned();
        p.push(".part");
        PathBuf::from(p)
    };

    let resume_from: u64 = if part_path.exists() {
        part_path.metadata().map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut req = client.get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if resume_from > 0 {
        req = req.header("Range", format!("bytes={}-", resume_from));
    }

    let mut response = req.send().with_context(|| format!("downloading {}", url))?;

    let status = response.status();
    if !status.is_success() {
        bail!("HTTP {} while downloading {}", status, url);
    }

    // 206 Partial Content = server supports resume; 200 = restart from scratch
    let actual_resume = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        resume_from
    } else {
        0
    };

    let filename = Path::new(url)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Downloading...".to_string());

    let content_length = response.content_length().unwrap_or(0);
    let total = actual_resume + content_length;

    let style = ProgressStyle::with_template(
        "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("=>-");

    let pb: ProgressBar = {
        let child = ProgressBar::new(total);
        child.set_style(style);
        if actual_resume > 0 {
            child.set_message(format!("{} (resuming)", filename));
            child.set_position(actual_resume);
        } else {
            child.set_message(filename);
        }
        if let Some(mp) = multi {
            mp.add(child)
        } else {
            child
        }
    };

    if let Some(parent) = part_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Append to .part if resuming, create/overwrite otherwise
    let mut file = if actual_resume > 0 {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .with_context(|| format!("opening {} for append", part_path.display()))?
    } else {
        std::fs::File::create(&part_path)
            .with_context(|| format!("creating {}", part_path.display()))?
    };

    let mut buf = [0u8; 65536];
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 { break; }
        file.write_all(&buf[..n])?;
        pb.inc(n as u64);
    }
    pb.finish_and_clear();

    // Rename .part → dest on successful completion
    std::fs::rename(&part_path, dest)
        .with_context(|| format!("renaming {} to {}", part_path.display(), dest.display()))?;

    if let Some(ov) = overall {
        ov.inc(1);
    }

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
        // Unknown format – try tar.gz first, then zip, then copy as-is
        if extract_tar_gz(src_path, dest_path).is_err()
            && extract_zip(src_path, dest_path).is_err()
        {
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
