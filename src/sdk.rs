use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::{
    config::Scope,
    paths::Paths,
    plugin::{EnvKeyItem, InstalledPackage, LuaPlugin, PreInstallResult},
};

// ═══════════════════════════════════════════════════════════════════════════════
// Resolved environment from all active SDKs
// ═══════════════════════════════════════════════════════════════════════════════

/// Collected environment variables from one or more SDKs.
#[derive(Debug, Default)]
pub struct SdkEnvs {
    pub paths: Vec<String>,
    pub vars:  HashMap<String, String>,
}

impl SdkEnvs {
    pub fn merge(&mut self, items: &[EnvKeyItem]) {
        for item in items {
            if item.key == "PATH" {
                self.paths.push(item.value.clone());
            } else {
                self.vars.insert(item.key.clone(), item.value.clone());
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sdk – manages one SDK (install, use, env resolution)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct Sdk<'a> {
    pub name:       String,
    pub plugin:     Arc<LuaPlugin>,
    pub paths:      &'a Paths,
    pub proxy_url:  Option<String>,
    pub ssl_verify: bool,
}

impl<'a> Sdk<'a> {
    pub fn new(name: String, plugin: Arc<LuaPlugin>, paths: &'a Paths, proxy_url: Option<String>, ssl_verify: bool) -> Self {
        Self { name, plugin, paths, proxy_url, ssl_verify }
    }

    // ── Version management ────────────────────────────────────────────────────

    /// All installed versions (newest first).
    pub fn installed_versions(&self) -> Vec<String> {
        self.paths.installed_versions(&self.name)
    }

    /// Whether a specific version is installed.
    pub fn is_installed(&self, version: &str) -> bool {
        self.paths.version_dir(&self.name, version).exists()
    }

    /// Resolve a version string:
    /// 1. Try exact match  
    /// 2. Ask `PreUse` hook  
    /// 3. Prefix-match installed versions  
    pub fn resolve_version(
        &self,
        version: &str,
        scope: Scope,
        previous: &str,
        cwd: &str,
    ) -> Result<String> {
        // Exact match short-circuits everything
        if self.is_installed(version) {
            return Ok(version.to_string());
        }

        let installed = self.installed_sdks();

        // Ask the plugin
        if let Some(result) = self
            .plugin
            .call_pre_use(version, previous, &scope_name(scope), cwd, installed.clone())
            .context("PreUse hook")?
        {
            if !result.version.is_empty() && self.is_installed(&result.version) {
                return Ok(result.version);
            }
        }

        // Prefix match (e.g. "20" matches "20.0.0")
        let prefix = format!("{}.", version);
        let mut versions = self.installed_versions();
        versions.sort();
        for v in &versions {
            if v == version || v.starts_with(&prefix) {
                return Ok(v.clone());
            }
        }

        Ok(version.to_string())
    }

    // ── Install ───────────────────────────────────────────────────────────────

    /// Download, extract and register a version.
    pub fn install(&self, version: &str) -> Result<String> {
        println!("Installing {}@{}", self.name.cyan(), version.green());

        let info = self
            .plugin
            .call_pre_install(version)
            .context("PreInstall hook")?;

        let resolved_version = info.version.clone();

        if self.is_installed(&resolved_version) {
            println!("{} is already installed", self.label(&resolved_version).green());
            return Ok(resolved_version);
        }

        let version_dir = self.paths.version_dir(&self.name, &resolved_version);
        std::fs::create_dir_all(&version_dir)?;

        let mut sdk_info: HashMap<String, InstalledPackage> = HashMap::new();

        // Install main runtime
        let _main_name = info.name.clone().unwrap_or_else(|| self.name.clone());
        let main_dir  = version_dir.join(format!("{}-{}", self.name, resolved_version));
        let main_path = self.install_package(&info, &main_dir).with_context(|| {
            // Clean up on failure
            let _ = std::fs::remove_dir_all(&version_dir);
            format!("installing main runtime {}", self.label(&resolved_version))
        })?;

        sdk_info.insert(
            self.name.clone(),
            InstalledPackage {
                name:    self.name.clone(),
                version: resolved_version.clone(),
                path:    main_path.to_string_lossy().to_string(),
                note:    info.note.clone(),
            },
        );

        // Install additional runtimes
        if !info.addition.is_empty() {
            println!(
                "There are {} additional package(s) to install...",
                info.addition.len()
            );
        }
        for addon in &info.addition {
            let addon_name = addon.name.clone().unwrap_or_else(|| "unknown".to_string());
            let addon_dir = version_dir.join(format!(
                "{}{}-{}",
                crate::paths::ADDITION_PREFIX,
                addon_name,
                addon.version
            ));
            let addon_path = self.install_package(addon, &addon_dir).with_context(|| {
                let _ = std::fs::remove_dir_all(&version_dir);
                format!("installing addon {}", addon_name)
            })?;
            sdk_info.insert(
                addon_name.clone(),
                InstalledPackage {
                    name:    addon_name,
                    version: addon.version.clone(),
                    path:    addon_path.to_string_lossy().to_string(),
                    note:    addon.note.clone(),
                },
            );
        }

        // PostInstall hook
        self.plugin
            .call_post_install(
                &version_dir.to_string_lossy(),
                sdk_info,
            )
            .context("PostInstall hook")?;

        println!(
            "Install {} success!",
            self.label(&resolved_version).green()
        );
        println!(
            "Please use `{}` to activate it.",
            format!("sdk use {}@{}", self.name, resolved_version).blue()
        );

        Ok(resolved_version)
    }

    fn install_package(&self, info: &PreInstallResult, dest_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dest_dir)?;

        if info.url.is_empty() {
            // No download needed (plugin handles everything in PostInstall)
            return Ok(dest_dir.to_owned());
        }

        if info.url.starts_with("https://") || info.url.starts_with("http://") {
            let tmp_file = self.paths.tmp.join(
                Path::new(&info.url).file_name().unwrap_or_default(),
            );
            crate::util::download_with_progress(&info.url, &info.headers, &tmp_file, self.proxy_url.as_deref(), self.ssl_verify)
                .context("downloading SDK")?;

            // Verify checksum
            let checksum = Checksum::from_result(info);
            if let Some(cs) = checksum {
                cs.verify(&tmp_file).context("checksum verification")?;
            }

            // Extract
            crate::util::extract(
                &tmp_file.to_string_lossy(),
                &dest_dir.to_string_lossy(),
            )
            .context("extracting archive")?;

            let _ = std::fs::remove_file(&tmp_file);
        } else {
            // Local file
            crate::util::extract(&info.url, &dest_dir.to_string_lossy())
                .context("extracting local archive")?;
        }

        Ok(dest_dir.to_owned())
    }

    // ── Uninstall ─────────────────────────────────────────────────────────────

    pub fn uninstall(&self, version: &str) -> Result<()> {
        if !self.is_installed(version) {
            bail!("{} is not installed", self.label(version).red());
        }

        let pkg = self.get_runtime_package(version)?;
        self.plugin
            .call_pre_uninstall(pkg.main.clone(), pkg.sdk_info.clone())
            .context("PreUninstall hook")?;

        std::fs::remove_dir_all(self.paths.version_dir(&self.name, version))
            .context("removing version directory")?;

        println!("Uninstalled {} successfully.", self.label(version).green());
        Ok(())
    }

    // ── Environment resolution (the key "no symlinks" optimization) ───────────

    /// Compute env vars for a version by calling the plugin's `EnvKeys` hook
    /// with the **actual install path** – no symlinks involved.
    pub fn env_keys_for_version(&self, version: &str) -> Result<Vec<EnvKeyItem>> {
        let pkg = self.get_runtime_package(version)?;
        self.plugin
            .call_env_keys(pkg.main, pkg.sdk_info)
            .context("EnvKeys hook")
    }

    // ── Runtime package ───────────────────────────────────────────────────────

    pub fn get_runtime_package(&self, version: &str) -> Result<RuntimePackage> {
        let version_dir = self.paths.version_dir(&self.name, version);
        if !version_dir.exists() {
            bail!(
                "{} is not installed (run `sdk install {}@{}`)",
                self.label(version).red(),
                self.name,
                version
            );
        }

        let main_dir = self.paths.runtime_path(&self.name, version);
        let main = InstalledPackage {
            name:    self.name.clone(),
            version: version.to_string(),
            path:    main_dir.to_string_lossy().to_string(),
            note:    String::new(),
        };

        let mut sdk_info: HashMap<String, InstalledPackage> = HashMap::new();
        sdk_info.insert(self.name.clone(), main.clone());

        // Discover additions
        if let Ok(entries) = std::fs::read_dir(&version_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(crate::paths::ADDITION_PREFIX) {
                    let stripped = &name[crate::paths::ADDITION_PREFIX.len()..];
                    // Parse "addonname-version" – find the last '-'
                    if let Some(pos) = stripped.rfind('-') {
                        let addon_name    = &stripped[..pos];
                        let addon_version = &stripped[pos + 1..];
                        sdk_info.insert(
                            addon_name.to_string(),
                            InstalledPackage {
                                name:    addon_name.to_string(),
                                version: addon_version.to_string(),
                                path:    entry.path().to_string_lossy().to_string(),
                                note:    String::new(),
                            },
                        );
                    }
                }
            }
        }

        Ok(RuntimePackage { main, sdk_info })
    }

    // ── Available ─────────────────────────────────────────────────────────────

    pub fn available(&self, args: &[String]) -> Result<Vec<crate::plugin::AvailableItem>> {
        self.plugin.call_available(args).context("Available hook")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    pub fn label(&self, version: &str) -> String {
        format!("{}@{}", self.name, version)
    }

    fn installed_sdks(&self) -> HashMap<String, InstalledPackage> {
        let mut map = HashMap::new();
        for version in self.installed_versions() {
            if let Ok(pkg) = self.get_runtime_package(&version) {
                map.insert(version, pkg.main);
            }
        }
        map
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// RuntimePackage
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct RuntimePackage {
    pub main:     InstalledPackage,
    pub sdk_info: HashMap<String, InstalledPackage>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Checksum
// ═══════════════════════════════════════════════════════════════════════════════

enum ChecksumType { Sha256, Sha512, Sha1, Md5 }

struct Checksum {
    kind:  ChecksumType,
    value: String,
}

impl Checksum {
    fn from_result(info: &PreInstallResult) -> Option<Self> {
        if !info.sha256.is_empty() {
            return Some(Self { kind: ChecksumType::Sha256, value: info.sha256.clone() });
        }
        if !info.sha512.is_empty() {
            return Some(Self { kind: ChecksumType::Sha512, value: info.sha512.clone() });
        }
        if !info.sha1.is_empty() {
            return Some(Self { kind: ChecksumType::Sha1, value: info.sha1.clone() });
        }
        if !info.md5.is_empty() {
            return Some(Self { kind: ChecksumType::Md5, value: info.md5.clone() });
        }
        None
    }

    fn verify(&self, path: &Path) -> Result<()> {
        use std::io::Read;

        let mut f = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;

        let computed = match self.kind {
            ChecksumType::Sha256 => {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(&buf);
                hex::encode(h.finalize())
            }
            ChecksumType::Sha512 => {
                use sha2::{Digest, Sha512};
                let mut h = Sha512::new();
                h.update(&buf);
                hex::encode(h.finalize())
            }
            ChecksumType::Sha1 => {
                use sha1::{Digest, Sha1};
                let mut h = Sha1::new();
                h.update(&buf);
                hex::encode(h.finalize())
            }
            ChecksumType::Md5 => {
                use md5::{Digest, Md5};
                let mut h = Md5::new();
                h.update(&buf);
                hex::encode(h.finalize())
            }
        };

        let expected = self.value.to_lowercase();
        if computed != expected {
            bail!(
                "Checksum mismatch!\n  expected: {}\n  computed: {}",
                expected,
                computed
            );
        }
        println!("Checksum verified ✓");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

pub fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Global  => "global",
        Scope::Project => "project",
        Scope::Session => "session",
    }
}
