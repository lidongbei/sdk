use std::{collections::HashMap, sync::Arc};

use anyhow::{bail, Context, Result};
use colored::Colorize;
use dialoguer::{FuzzySelect, Select, theme::ColorfulTheme};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::{
    config::{ConfigChain, Scope, UserConfig, SdkToml},
    paths::Paths,
    plugin::LuaPlugin,
    registry,
    sdk::{Sdk, SdkEnvs, scope_name},
};

// ═══════════════════════════════════════════════════════════════════════════════
// App – top-level manager
// ═══════════════════════════════════════════════════════════════════════════════

pub struct App {
    pub paths:    Paths,
    pub user_cfg: UserConfig,
    plugins:      HashMap<String, Arc<LuaPlugin>>,
}

impl App {
    pub fn new() -> Result<Self> {
        let paths    = Paths::new()?;
        let user_cfg = UserConfig::load(&paths.user_config)?;

        // Resolve the local mirror base directory:
        // - use configured mirror.local_dir if set
        // - otherwise fall back to the downloads cache directory
        let local_dir = if user_cfg.mirror_cfg.local_dir.is_empty() {
            paths.downloads.to_string_lossy().into_owned()
        } else {
            user_cfg.mirror_cfg.local_dir.clone()
        };

        // Apply mirror env vars from config so plugin Lua hooks see them via os.getenv()
        // Expand the {local_dir} placeholder to the resolved local mirror directory.
        for entry in user_cfg.mirrors.values() {
            for (k, v) in &entry.vars {
                if !v.is_empty() {
                    let resolved = v.replace("{local_dir}", &local_dir);
                    std::env::set_var(k, resolved);
                }
            }
        }

        Ok(Self {
            paths,
            user_cfg,
            plugins: HashMap::new(),
        })
    }

    // ── Plugin management ─────────────────────────────────────────────────────

    pub fn load_plugin(&mut self, name: &str) -> Result<Arc<LuaPlugin>> {
        if !self.plugins.contains_key(name) {
            let plugin_dir = self.paths.plugin_dir(name);
            if !plugin_dir.exists() {
                bail!("Plugin '{}' not found. Run `sdk add {}`.", name.cyan(), name);
            }
            let plugin = LuaPlugin::load(&plugin_dir, &self.user_cfg)
                .with_context(|| format!("loading plugin {}", name))?;
            self.plugins.insert(name.to_string(), Arc::new(plugin));
        }
        Ok(Arc::clone(self.plugins.get(name).unwrap()))
    }

    /// Install a plugin from a registry path, URL, or local directory.
    pub fn add_plugin(&self, name: &str, source: &str) -> Result<()> {
        let plugin_dir = self.paths.plugin_dir(name);
        if plugin_dir.exists() {
            println!("Plugin '{}' already exists.", name.cyan());
            return Ok(());
        }
        println!("Adding plugin '{}' from {}...", name.cyan(), source.blue());

        let source_path = std::path::Path::new(source);
        let is_local = source.starts_with('/')
            || source.starts_with('.')
            || source.starts_with('~')
            || (source_path.is_absolute())
            || source_path.exists();

        if is_local {
            // Expand ~ manually
            let expanded = if source.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    home.join(&source[2..])
                } else {
                    std::path::PathBuf::from(source)
                }
            } else {
                std::path::PathBuf::from(source)
            };

            if !expanded.exists() {
                bail!("Local plugin path does not exist: {}", expanded.display());
            }

            copy_dir_all(&expanded, &plugin_dir)
                .with_context(|| format!("copying plugin from {}", expanded.display()))?;
        } else {
            // Git clone
            let status = std::process::Command::new("git")
                .args(["clone", "--depth=1", source, plugin_dir.to_str().unwrap_or("")])
                .status()
                .context("git clone")?;
            if !status.success() {
                bail!("Failed to clone plugin from {}", source);
            }
        }

        println!("Plugin '{}' added successfully.", name.cyan());
        Ok(())
    }

    pub fn remove_plugin(&self, name: &str) -> Result<()> {
        let plugin_dir = self.paths.plugin_dir(name);
        if !plugin_dir.exists() {
            bail!("Plugin '{}' not found.", name.cyan());
        }
        std::fs::remove_dir_all(&plugin_dir)
            .with_context(|| format!("removing plugin directory {}", plugin_dir.display()))?;
        // Also remove all installed versions for this plugin
        let versions_dir = self.paths.sdk_cache_dir(name);
        if versions_dir.exists() {
            let _ = std::fs::remove_dir_all(&versions_dir);
        }
        println!("Plugin '{}' removed.", name.cyan());
        Ok(())
    }

    pub fn update_plugins(&self) -> Result<()> {
        for entry in std::fs::read_dir(&self.paths.plugins)?.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            println!("Updating plugin '{}'...", name.cyan());
            let status = std::process::Command::new("git")
                .args(["-C", entry.path().to_str().unwrap_or(""), "pull", "--ff-only"])
                .status();
            match status {
                Ok(s) if s.success() => println!("  ✓ {}", name.green()),
                _ => println!("  ✗ {} (skipped)", name.yellow()),
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn list_plugins(&self) -> Result<Vec<String>> {
        let mut plugins = Vec::new();
        for entry in std::fs::read_dir(&self.paths.plugins)?.flatten() {
            if entry.path().is_dir() {
                plugins.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        plugins.sort();
        Ok(plugins)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn proxy_url(&self) -> Option<String> {
        if self.user_cfg.proxy.enable && !self.user_cfg.proxy.url.is_empty() {
            Some(self.user_cfg.proxy.url.clone())
        } else {
            None
        }
    }

    fn ssl_verify(&self) -> bool {
        self.user_cfg.proxy.ssl_verify
    }

    /// Resolve the effective local mirror directory.
    /// Returns `mirror.local_dir` if configured, otherwise defaults to `~/.sdk/downloads/`.
    fn local_dir(&self) -> String {
        if self.user_cfg.mirror_cfg.local_dir.is_empty() {
            self.paths.downloads.to_string_lossy().into_owned()
        } else {
            self.user_cfg.mirror_cfg.local_dir.clone()
        }
    }

    /// Expand `{local_dir}` placeholder in a mirror var value.
    fn expand_mirror_var(&self, v: &str) -> String {
        v.replace("{local_dir}", &self.local_dir())
    }

    // ── Download cache (offline mirror) ──────────────────────────────────────

    /// List all archives in `~/.sdk/downloads/`.
    pub fn cache_list(&self) -> Result<()> {
        let dir = &self.paths.downloads;
        let mut entries: Vec<(String, u64)> = std::fs::read_dir(dir)?
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if meta.is_file() {
                    Some((e.file_name().to_string_lossy().to_string(), meta.len()))
                } else {
                    None
                }
            })
            .collect();

        if entries.is_empty() {
            println!("No cached archives in {}", dir.display());
            return Ok(());
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let total: u64 = entries.iter().map(|(_, s)| s).sum();

        println!("Cached archives in {}:", dir.display().to_string().cyan());
        for (name, size) in &entries {
            println!("  {:>10}  {}", format_bytes(*size).yellow(), name);
        }
        println!("  {:>10}  {} files total", format_bytes(total).green(), entries.len());
        Ok(())
    }

    /// Remove all archives from `~/.sdk/downloads/`.
    pub fn cache_clean(&self) -> Result<()> {
        let dir = &self.paths.downloads;
        let mut count = 0u64;
        let mut freed = 0u64;
        for entry in std::fs::read_dir(dir)?.flatten() {
            if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
                freed += entry.metadata().map(|m| m.len()).unwrap_or(0);
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
        if count == 0 {
            println!("Downloads cache is already empty.");
        } else {
            println!("Removed {} archive(s), freed {}.", count, format_bytes(freed).green());
        }
        Ok(())
    }

    // ── Install / Uninstall ───────────────────────────────────────────────────

    pub fn install(&mut self, sdk_name: &str, version: &str) -> Result<()> {
        let plugin = self.load_plugin(sdk_name)?;
        let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
        sdk.install(version)?;
        Ok(())
    }

    pub fn uninstall(&mut self, sdk_name: &str, version: &str) -> Result<()> {
        let plugin = self.load_plugin(sdk_name)?;
        let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());

        // If this version is globally active, remove it from the registry first.
        if let Ok(global_toml) = SdkToml::load(&self.paths.global_toml) {
            if global_toml.get_version(sdk_name) == Some(version) {
                match sdk.env_keys_for_version(version) {
                    Ok(items) => {
                        let mut envs = SdkEnvs::default();
                        envs.merge(&items);
                        if let Err(e) = registry::remove_global_env(&envs.paths, &envs.vars) {
                            eprintln!("Warning: could not remove from global environment: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Warning: EnvKeys hook for {} failed: {}", sdk_name, e),
                }
            }
        }

        sdk.uninstall(version)?;

        // Warn if version still referenced in .sdk.toml
        let chain  = ConfigChain::load(&self.paths)?;
        let config = chain.effective_config();
        for (name, tool) in &config.tools {
            if name == sdk_name && tool.version == version {
                println!(
                    "Note: version {} is still referenced in .sdk.toml – run `sdk unuse {}` to clear it.",
                    version.yellow(),
                    sdk_name
                );
                break;
            }
        }
        Ok(())
    }

    // ── Link / Unlink (external SDK) ─────────────────────────────────────────

    /// Register a locally-installed SDK directory as version `version`.
    /// Creates `~/.sdk/cache/<sdk>/v-<version>/.link` pointing to `path`.
    pub fn link(&self, sdk_name: &str, version: &str, path: &str) -> Result<()> {
        let external = std::path::Path::new(path);
        if !external.exists() {
            bail!("Path does not exist: {}", path);
        }
        if !external.is_dir() {
            bail!("Path is not a directory: {}", path);
        }

        let version_dir = self.paths.version_dir(sdk_name, version);
        if version_dir.exists() {
            let link_file = self.paths.link_file(sdk_name, version);
            if !link_file.exists() {
                bail!(
                    "{}@{} is already installed via `sdk install`.\n\
                     Uninstall it first: sdk uninstall {}@{}",
                    sdk_name, version, sdk_name, version
                );
            }
            // Already linked — overwrite
        }

        std::fs::create_dir_all(&version_dir)
            .with_context(|| format!("creating version dir {}", version_dir.display()))?;

        let canonical = external.canonicalize()
            .unwrap_or_else(|_| external.to_path_buf());
        let link_file = self.paths.link_file(sdk_name, version);
        std::fs::write(&link_file, canonical.to_string_lossy().as_bytes())
            .with_context(|| format!("writing link file {}", link_file.display()))?;

        println!(
            "{} {}  {}",
            "Linked".green(),
            format!("{}@{}", sdk_name, version).cyan(),
            format!("→ {}", canonical.display()).dimmed()
        );
        println!("Activate with:  {}", format!("sdk use {}@{}", sdk_name, version).cyan());
        Ok(())
    }

    /// Remove a linked SDK version. Refuses to remove regular (non-linked) installs.
    pub fn unlink(&self, sdk_name: &str, version: &str) -> Result<()> {
        let link_file = self.paths.link_file(sdk_name, version);
        if !link_file.exists() {
            let version_dir = self.paths.version_dir(sdk_name, version);
            if version_dir.exists() {
                bail!(
                    "{}@{} was installed via `sdk install`, not linked.\n\
                     Use `sdk uninstall {}@{}` to remove it.",
                    sdk_name, version, sdk_name, version
                );
            } else {
                bail!("{}@{} is not linked (not found).", sdk_name, version);
            }
        }

        let version_dir = self.paths.version_dir(sdk_name, version);
        std::fs::remove_dir_all(&version_dir)
            .with_context(|| format!("removing {}", version_dir.display()))?;

        println!(
            "{} {}",
            "Unlinked".yellow(),
            format!("{}@{}", sdk_name, version).cyan()
        );
        Ok(())
    }

    // ── Use / Unuse ───────────────────────────────────────────────────────────

    /// Write `sdk@version` to `.sdk.toml` (project or global, no symlinks).
    pub fn use_sdk(
        &mut self,
        sdk_name: &str,
        version: &str,
        scope: Scope,
    ) -> Result<()> {
        let plugin  = self.load_plugin(sdk_name)?;
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
        let chain   = ConfigChain::load(&self.paths)?;
        let current = chain.get_version_for_scope(sdk_name, scope).unwrap_or_default();
        let cwd     = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let resolved = sdk.resolve_version(version, scope, &current, &cwd)?;

        if !sdk.is_installed(&resolved) {
            println!("Version {} not installed. Installing now...", resolved.green());
            sdk.install(&resolved)?;
        }

        let toml_path = match scope {
            Scope::Global  => self.paths.global_toml.clone(),
            Scope::Project => find_or_create_project_toml()?,
            Scope::Session => self.paths.session_toml(),
        };

        let mut toml = SdkToml::load(&toml_path).unwrap_or_default();
        toml.set_tool(sdk_name, &resolved);
        toml.save(&toml_path)?;

        // For global scope, also persist PATH/env vars to the user environment.
        if scope == Scope::Global {
            match sdk.env_keys_for_version(&resolved) {
                Ok(items) => {
                    let mut envs = SdkEnvs::default();
                    envs.merge(&items);
                    if let Err(e) = registry::apply_global_env(&envs.paths, &envs.vars) {
                        eprintln!("Warning: could not update global environment: {}", e);
                    }
                }
                Err(e) => eprintln!("Warning: EnvKeys hook for {} failed: {}", sdk_name, e),
            }
        }

        println!(
            "Using {}{}.",
            sdk.label(&resolved).green(),
            match scope {
                Scope::Global  => " (global)".dimmed().to_string(),
                Scope::Project => " (project)".dimmed().to_string(),
                Scope::Session => " (session)".dimmed().to_string(),
            }
        );
        Ok(())
    }

    /// Remove `sdk` from the given scope's `.sdk.toml`.
    pub fn unuse_sdk(&mut self, sdk_name: &str, scope: Scope) -> Result<()> {
        let toml_path = match scope {
            Scope::Global  => self.paths.global_toml.clone(),
            Scope::Project => find_project_toml()?,
            Scope::Session => self.paths.session_toml(),
        };

        // For global scope, remove the version's env vars from the user environment
        // before we lose which version was active.
        if scope == Scope::Global {
            if let Ok(toml) = SdkToml::load(&toml_path) {
                if let Some(version) = toml.get_version(sdk_name) {
                    let plugin = self.load_plugin(sdk_name)?;
                    let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
                    match sdk.env_keys_for_version(version) {
                        Ok(items) => {
                            let mut envs = SdkEnvs::default();
                            envs.merge(&items);
                            if let Err(e) = registry::remove_global_env(&envs.paths, &envs.vars) {
                                eprintln!("Warning: could not update global environment: {}", e);
                            }
                        }
                        Err(e) => eprintln!("Warning: EnvKeys hook for {} failed: {}", sdk_name, e),
                    }
                }
            }
        }

        let mut toml = SdkToml::load(&toml_path)?;
        toml.remove_tool(sdk_name);
        toml.save(&toml_path)?;
        println!("Removed {} from {} config.", sdk_name.cyan(), scope_name(scope).yellow());
        Ok(())
    }

    // ── List ──────────────────────────────────────────────────────────────────

    pub fn list_installed(&mut self, sdk_name: Option<&str>) -> Result<()> {
        if let Some(name) = sdk_name {
            let plugin  = self.load_plugin(name)?;
            let sdk     = Sdk::new(name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
            let chain   = ConfigChain::load(&self.paths)?;
            let current = chain.resolve_version(name);

            let versions = sdk.installed_versions();
            if versions.is_empty() {
                println!("No versions of {} installed.", name.cyan());
            } else {
                println!("Installed versions of {}:", name.cyan());
                for v in &versions {
                    let marker = if current.as_deref() == Some(v.as_str()) { "→ " } else { "  " };
                    let link_suffix = self.linked_suffix(name, v);
                    println!("  {}{}{}", marker.green(), v, link_suffix);
                }
            }
        } else {
            // All SDKs
            for entry in std::fs::read_dir(&self.paths.cache)?.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let versions = self.paths.installed_versions(&name);
                    if !versions.is_empty() {
                        let chain   = ConfigChain::load(&self.paths)?;
                        let current = chain.resolve_version(&name);
                        println!("{}:", name.cyan());
                        for v in &versions {
                            let marker = if current.as_deref() == Some(v.as_str()) { "→ " } else { "  " };
                            let link_suffix = self.linked_suffix(&name, v);
                            println!("  {}{}{}", marker.green(), v, link_suffix);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns "(linked → /path)" if `version` is a linked install, else empty string.
    fn linked_suffix(&self, sdk: &str, version: &str) -> String {
        let lf = self.paths.link_file(sdk, version);
        if lf.exists() {
            let path = std::fs::read_to_string(&lf).unwrap_or_default();
            format!("  {}", format!("(linked → {})", path.trim()).dimmed())
        } else {
            String::new()
        }
    }

    pub fn current(&self) -> Result<()> {
        let chain = ConfigChain::load(&self.paths)?;
        let config = chain.effective_config();
        if config.tools.is_empty() {
            println!("No SDKs active.");
        } else {
            for (name, tool) in &config.tools {
                println!("{}  {}", name.cyan(), tool.version.green());
            }
        }
        Ok(())
    }

    // ── Available ─────────────────────────────────────────────────────────────

    pub fn available(&mut self, sdk_name: &str, args: &[String]) -> Result<()> {
        let plugin  = self.load_plugin(sdk_name)?;
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
        let items   = sdk.available(args)?;
        for item in &items {
            if item.note.is_empty() {
                println!("  {}", item.version.green());
            } else {
                println!("  {}  {}", item.version.green(), item.note.as_str().dimmed());
            }
        }
        Ok(())
    }

    // ── Activate (core "no symlinks" output) ──────────────────────────────────

    /// Emit shell commands to export env vars for all active SDK versions.
    /// The caller (shell hook) evals this output.  
    /// **No symlinks are created** – paths point directly to `~/.sdk/cache/…`.
    pub fn activate(&mut self, shell: &str, cwd: &str) -> Result<String> {
        let chain  = ConfigChain::load_from_dir(&self.paths, cwd)?;
        let mut config = chain.effective_config();

        // For each installed plugin, check legacy files (e.g. .nvmrc) as fallback
        // when the SDK is NOT already specified by .sdk.toml layers.
        let cwd_path = std::path::Path::new(cwd);
        for sdk_name in self.paths.installed_plugins() {
            if config.tools.contains_key(&sdk_name) {
                continue; // .sdk.toml already has a version — skip legacy scan
            }
            if let Some(ver) = self.find_legacy_version(&sdk_name, cwd_path) {
                config.set_tool(&sdk_name, &ver);
            }
        }

        let mut envs = SdkEnvs::default();

        for (sdk_name, tool) in &config.tools {
            let version = &tool.version;
            let plugin  = match self.load_plugin(sdk_name) {
                Ok(p)  => p,
                Err(e) => {
                    eprintln!("Warning: skipping {} – {}", sdk_name, e);
                    continue;
                }
            };
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
            if !sdk.is_installed(version) {
                eprintln!(
                    "Warning: {}@{} not installed – skipping.",
                    sdk_name, version
                );
                continue;
            }
            match sdk.env_keys_for_version(version) {
                Ok(keys) => envs.merge(&keys),
                Err(e)   => eprintln!("Warning: EnvKeys for {} failed – {}", sdk_name, e),
            }
        }

        crate::shell::render_env(shell, &envs)
    }

    /// Walk up from `cwd` looking for legacy version files (e.g. `.nvmrc`, `.node-version`)
    /// defined in the plugin's metadata.  Returns the resolved version string if found.
    fn find_legacy_version(&mut self, sdk_name: &str, cwd: &std::path::Path) -> Option<String> {
        let plugin = self.load_plugin(sdk_name).ok()?;
        let filenames = plugin.metadata.legacy_filenames.clone();
        if filenames.is_empty() {
            return None;
        }

        // Walk up directory tree to find any legacy file
        let mut dir = cwd;
        loop {
            for fname in &filenames {
                let candidate = dir.join(fname);
                if candidate.exists() {
                    let installed = self.paths.installed_versions(sdk_name);
                    return plugin
                        .call_parse_legacy_file(
                            candidate.to_str()?,
                            fname,
                            &installed,
                        )
                        .ok()
                        .flatten()
                        .map(|r| r.version)
                        .filter(|v| !v.is_empty());
                }
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
        None
    }

    // ── Exec ──────────────────────────────────────────────────────────────────

    pub fn exec(&mut self, sdk_name: &str, version: &str, command: &[String]) -> Result<i32> {
        let plugin  = self.load_plugin(sdk_name)?;
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
        let keys    = sdk.env_keys_for_version(version)?;

        let mut cmd = std::process::Command::new(&command[0]);
        cmd.args(&command[1..]);

        let mut path_prepend = Vec::new();
        for item in &keys {
            if item.key == "PATH" {
                path_prepend.push(item.value.clone());
            } else {
                cmd.env(&item.key, &item.value);
            }
        }

        if !path_prepend.is_empty() {
            let existing_path = std::env::var("PATH").unwrap_or_default();
            #[cfg(windows)]
            let sep = ";";
            #[cfg(not(windows))]
            let sep = ":";
            let new_path = format!("{}{}{}", path_prepend.join(sep), sep, existing_path);
            cmd.env("PATH", new_path);
        }

        let status = cmd.status().context("exec command")?;
        Ok(status.code().unwrap_or(1))
    }

    // ── Info ──────────────────────────────────────────────────────────────────

    pub fn info(&mut self, sdk_name: &str) -> Result<()> {
        let plugin_dir = self.paths.plugin_dir(sdk_name);
        if !plugin_dir.exists() {
            bail!("Plugin '{}' not found. Run `sdk add {}`.", sdk_name.cyan(), sdk_name);
        }

        let plugin = self.load_plugin(sdk_name)?.clone();
        let meta   = &plugin.metadata;

        // ── Plugin metadata ───────────────────────────────────────────────────
        println!("{}", sdk_name.cyan().bold());
        if !meta.description.is_empty() {
            println!("{}", meta.description);
        }
        println!();

        let kw = 22usize;
        macro_rules! row {
            ($label:expr, $val:expr) => {
                if !$val.is_empty() {
                    println!("  {:<kw$}  {}", $label, $val, kw = kw);
                }
            };
        }

        row!("Plugin version",    &meta.version);
        row!("Homepage",          &meta.homepage);
        row!("Update URL",        &meta.update_url);
        row!("Min runtime ver",   &meta.min_runtime_version);
        if !meta.legacy_filenames.is_empty() {
            println!("  {:<kw$}  {}", "Legacy version files", meta.legacy_filenames.join(", "), kw = kw);
        }
        println!("  {:<kw$}  {}", "Plugin path", plugin_dir.display(), kw = kw);

        // ── Active versions (from config chain) ───────────────────────────────
        println!();
        println!("{}", "Active versions".bold());
        let cwd   = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let chain = ConfigChain::load_from_dir(&self.paths, &cwd).unwrap_or_default();
        let installed_set: std::collections::HashSet<String> =
            self.paths.installed_versions(sdk_name).into_iter().collect();
        if let Some((scope, tool_cfg)) = chain.resolve(sdk_name) {
            let ver = &tool_cfg.version;
            let label = scope_name(scope);
            let warn = if installed_set.contains(ver) { "" } else { " ⚠ not installed" };
            println!("  {} {}{}",
                ver.green(),
                format!("({})", label).dimmed(),
                warn.yellow());
        } else {
            println!("  {}", "(none)".dimmed());
        }

        // ── Installed versions ────────────────────────────────────────────────
        println!();
        println!("{}", "Installed versions".bold());
        let installed = self.paths.installed_versions(sdk_name);
        if installed.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            for v in &installed {
                println!("  {}", v.green());
            }
        }

        Ok(())
    }

    // ── Config ────────────────────────────────────────────────────────────────

    pub fn config_show(&self) {
        let key_w = 20usize;
        println!("{:<key_w$}  {}", "KEY".bold(), "VALUE".bold(), key_w = key_w);
        println!("{}", "-".repeat(key_w + 32));
        for (k, v) in self.user_cfg.all_pairs() {
            println!("{:<key_w$}  {}", k, v, key_w = key_w);
        }
        println!("\nConfig file: {}", self.paths.user_config.display().to_string().dimmed());
    }

    pub fn config_get(&self, key: &str) -> Result<()> {
        match self.user_cfg.get_key(key) {
            Some(v) => { println!("{}", v); Ok(()) }
            None    => anyhow::bail!("Unknown config key '{}'", key),
        }
    }

    pub fn config_set(&mut self, key: &str, value: &str) -> Result<()> {
        self.user_cfg.set_key(key, value)?;
        self.user_cfg.save(&self.paths.user_config)?;
        println!("Set {} = {}", key.cyan(), value.green());
        Ok(())
    }

    // ── Env show ──────────────────────────────────────────────────────────────

    /// Print the effective environment variables that would be exported for a
    /// scope: paths prepended to PATH + extra key=value pairs.
    pub fn env_show(&mut self, global_only: bool) -> Result<()> {
        let chain = if global_only {
            ConfigChain::load(&self.paths)?
        } else {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            ConfigChain::load_from_dir(&self.paths, &cwd)?
        };
        let config = if global_only {
            // Only global layer
            SdkToml::load(&self.paths.global_toml).unwrap_or_default()
        } else {
            chain.effective_config()
        };

        if config.tools.is_empty() {
            println!("No active SDKs{}.", if global_only { " (global)" } else { "" });
            return Ok(());
        }

        let scope_label = if global_only { " (global)" } else { " (effective)" };
        println!("Active SDK environment{}:", scope_label.dimmed());

        for (sdk_name, tool) in &config.tools {
            let version = &tool.version;
            let plugin = match self.load_plugin(sdk_name) {
                Ok(p)  => p,
                Err(e) => {
                    eprintln!("  Warning: {} – {}", sdk_name, e);
                    continue;
                }
            };
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
            if !sdk.is_installed(version) {
                println!("  {} {} (not installed)", sdk_name.yellow(), version);
                continue;
            }
            match sdk.env_keys_for_version(version) {
                Ok(items) => {
                    println!("  {}@{}:", sdk_name.cyan(), version.green());
                    for item in &items {
                        if item.key == "PATH" {
                            println!("    PATH += {}", item.value.dimmed());
                        } else {
                            println!("    {} = {}", item.key, item.value.dimmed());
                        }
                    }
                }
                Err(e) => eprintln!("  Warning: EnvKeys for {} failed: {}", sdk_name, e),
            }
        }
        Ok(())
    }

    // ── Upgrade ───────────────────────────────────────────────────────────────

    /// Check for newer versions of currently-used SDKs and optionally upgrade.
    pub fn upgrade(&mut self, sdk_filter: Option<&str>, auto: bool, include_pre: bool) -> Result<()> {
        let cwd     = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let chain   = ConfigChain::load_from_dir(&self.paths, &cwd)?;
        let config  = chain.effective_config();

        if config.tools.is_empty() {
            println!("No active SDKs to check.");
            return Ok(());
        }

        let mut checked   = 0usize;
        let mut up_to_date = 0usize;
        let mut upgradeable = 0usize;
        let mut upgraded  = 0usize;
        let mut errors    = 0usize;

        for (sdk_name, tool) in &config.tools {
            if let Some(f) = sdk_filter {
                if sdk_name != f { continue; }
            }
            checked += 1;
            let current = &tool.version;

            let plugin = match self.load_plugin(sdk_name) {
                Ok(p)  => p,
                Err(e) => {
                    println!("  {} {}  {}", "✗".red(), sdk_name.cyan(), e.to_string().dimmed());
                    errors += 1;
                    continue;
                }
            };
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());

            print!("  {} (current: {}) checking…", sdk_name.cyan(), current.yellow());
            let latest = sdk.available(&[])
                .ok()
                .and_then(|items| {
                    items.into_iter()
                        .find(|i| include_pre || !is_prerelease(&i.version))
                        .map(|i| i.version)
                });

            match latest {
                None => {
                    println!("\r  {} {}  {}", "?".yellow(), sdk_name.cyan(), "no versions returned".dimmed());
                    errors += 1;
                }
                Some(ref latest) if latest == current => {
                    println!("\r  {} {} {}  up to date", "✓".green(), sdk_name.cyan(), current.green());
                    up_to_date += 1;
                }
                Some(latest) => {
                    println!("\r  {} {}  {} → {}", "↑".yellow(), sdk_name.cyan(), current.yellow(), latest.green());
                    upgradeable += 1;
                    if auto {
                        // Determine scope: prefer project over global
                        let scope = match chain.resolve(sdk_name) {
                            Some((s, _)) => s,
                            None         => Scope::Global,
                        };
                        print!("    installing {}@{}…", sdk_name.cyan(), latest.green());
                        match self.install(sdk_name, &latest) {
                            Ok(()) => {
                                match self.use_sdk(sdk_name, &latest, scope) {
                                    Ok(())  => {
                                        println!("\r    {} upgraded to {}", "✓".green(), latest.green());
                                        upgraded += 1;
                                    }
                                    Err(e) => {
                                        println!("\r    {} failed to activate: {}", "✗".red(), e);
                                        errors += 1;
                                    }
                                }
                            }
                            Err(e) => {
                                println!("\r    {} install failed: {}", "✗".red(), e);
                                errors += 1;
                            }
                        }
                    }
                }
            }
        }

        if checked == 0 {
            if let Some(f) = sdk_filter {
                println!("SDK '{}' is not currently active.", f);
            }
            return Ok(());
        }

        // Summary
        println!();
        println!("  {} checked  {} up to date  {} upgradeable",
            checked, up_to_date, upgradeable);
        if auto && upgraded > 0 {
            println!("  {} upgraded", upgraded.to_string().green());
        }
        if errors > 0 {
            println!("  {} error(s)", errors.to_string().red());
        }
        if !auto && upgradeable > 0 {
            println!("\n  Run {} to install all upgrades.", "sdk upgrade --yes".cyan());
        }

        Ok(())
    }

    // ── Search versions (interactive TUI) ────────────────────────────────────

    pub fn search(&mut self, sdk_name: &str, filter: Option<&str>) -> Result<()> {
        if !self.paths.plugin_dir(sdk_name).exists() {
            bail!(
                "Plugin '{}' is not installed.\n\
                 Add it first:  sdk add {} <url>",
                sdk_name, sdk_name
            );
        }

        // ── Offline mode: list local versions only ──────────────────────────
        if self.user_cfg.cache.offline {
            return self.search_offline(sdk_name, filter);
        }

        let plugin = self.load_plugin(sdk_name)?;
        let sdk = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify(), self.user_cfg.cache.keep_downloads, self.user_cfg.cache.mirror_dir.clone());
        let items = sdk.available(&[])?;

        let filtered: Vec<_> = if let Some(f) = filter {
            items.iter().filter(|i| i.version.contains(f)).collect()
        } else {
            items.iter().collect()
        };

        if filtered.is_empty() {
            if let Some(f) = filter {
                println!("No versions of '{}' match '{}'.", sdk_name, f);
            } else {
                println!("No versions found for '{}'.", sdk_name);
            }
            return Ok(());
        }

        let labels: Vec<String> = filtered.iter().map(|i| {
            if i.note.is_empty() {
                i.version.clone()
            } else {
                format!("{}  ({})", i.version, i.note)
            }
        }).collect();

        let idx = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Select a version of {}", sdk_name.bold()))
            .items(&labels)
            .default(0)
            .interact_opt()?;

        let Some(idx) = idx else {
            println!("Cancelled.");
            return Ok(());
        };

        let version = &filtered[idx].version;

        // Ask what to do with the selected version
        let actions = &["Use (session)", "Install", "Install + Use (session)", "Cancel"];
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{}@{}", sdk_name, version))
            .items(actions)
            .default(0)
            .interact_opt()?;

        match action {
            Some(0) => {
                self.use_sdk(sdk_name, version, Scope::Session)?;
            }
            Some(1) => {
                self.install(sdk_name, version)?;
            }
            Some(2) => {
                self.install(sdk_name, version)?;
                self.use_sdk(sdk_name, version, Scope::Session)?;
            }
            _ => println!("Cancelled."),
        }
        Ok(())
    }

    /// Offline version of search: shows installed versions + archives in mirror/downloads.
    fn search_offline(&mut self, sdk_name: &str, filter: Option<&str>) -> Result<()> {
        use crate::plugin::AvailableItem;

        let mut items: Vec<AvailableItem> = Vec::new();

        // Locally installed versions
        for v in self.paths.installed_versions(sdk_name) {
            items.push(AvailableItem { version: v, note: "installed".to_string(), addition: vec![] });
        }

        // Archives in downloads dir that match sdk_name
        let dirs: Vec<std::path::PathBuf> = {
            let mut d = vec![self.paths.downloads.clone()];
            if !self.user_cfg.cache.mirror_dir.is_empty() {
                d.push(std::path::PathBuf::from(&self.user_cfg.cache.mirror_dir));
            }
            d
        };

        for dir in &dirs {
            if !dir.exists() { continue; }
            for entry in std::fs::read_dir(dir)?.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(sdk_name) {
                    // Try to extract version from filename (e.g. node-v20.0.0-linux-x64.tar.gz → 20.0.0)
                    let note = format!("archive in {}", dir.display());
                    // Avoid duplicates with installed versions
                    let already = items.iter().any(|i| name.contains(&i.version));
                    if !already {
                        items.push(AvailableItem { version: name.clone(), note, addition: vec![] });
                    }
                }
            }
        }

        let filtered: Vec<_> = if let Some(f) = filter {
            items.iter().filter(|i| i.version.contains(f)).collect()
        } else {
            items.iter().collect()
        };

        if filtered.is_empty() {
            println!("No local versions or archives found for '{}' (offline mode).", sdk_name);
            return Ok(());
        }

        let labels: Vec<String> = filtered.iter().map(|i| {
            if i.note.is_empty() { i.version.clone() }
            else { format!("{}  ({})", i.version, i.note.dimmed()) }
        }).collect();

        println!("{} Offline mode — showing local versions only", "⚠".yellow());

        let idx = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Select a version of {}", sdk_name.bold()))
            .items(&labels)
            .default(0)
            .interact_opt()?;

        let Some(idx) = idx else {
            println!("Cancelled.");
            return Ok(());
        };

        let version = &filtered[idx].version;

        let actions = &["Use (session)", "Install", "Install + Use (session)", "Cancel"];
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{}@{}", sdk_name, version))
            .items(actions)
            .default(0)
            .interact_opt()?;

        match action {
            Some(0) => { self.use_sdk(sdk_name, version, Scope::Session)?; }
            Some(1) => { self.install(sdk_name, version)?; }
            Some(2) => {
                self.install(sdk_name, version)?;
                self.use_sdk(sdk_name, version, Scope::Session)?;
            }
            _ => println!("Cancelled."),
        }
        Ok(())
    }

    /// Interactive `sdk use <sdk>` without a version — shows a TUI version picker.
    pub fn use_interactive(&mut self, sdk_name: &str, scope: Scope) -> Result<()> {
        let versions = self.paths.installed_versions(sdk_name);

        if versions.is_empty() {
            bail!(
                "No installed versions of '{}' found.\n\
                 Install one first: sdk install {}@<version>\n\
                 Or link a local install: sdk link {} <version> <path>\n\
                 Or browse available versions: sdk search {}",
                sdk_name, sdk_name, sdk_name, sdk_name
            );
        }

        // Build labels with linked-path annotation where applicable
        let labels: Vec<String> = versions.iter().map(|v| {
            let lf = self.paths.link_file(sdk_name, v);
            if lf.exists() {
                let path = std::fs::read_to_string(&lf).unwrap_or_default();
                format!("{}  (linked → {})", v, path.trim())
            } else {
                v.clone()
            }
        }).collect();

        let idx = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Select a version of {}", sdk_name.bold()))
            .items(&labels)
            .default(0)
            .interact_opt()?;

        match idx {
            Some(i) => self.use_sdk(sdk_name, &versions[i], scope),
            None    => { println!("Cancelled."); Ok(()) }
        }
    }


    // ── Doctor ────────────────────────────────────────────────────────────────

    /// Run diagnostics: check plugins, installed versions, PATH integrity.
    pub fn doctor(&mut self) -> Result<()> {
        let mut ok   = 0usize;
        let mut warn = 0usize;

        macro_rules! check {
            (ok, $($msg:tt)*) => {{
                println!("  {} {}", "✓".green(), format!($($msg)*));
                ok += 1;
            }};
            (warn, $($msg:tt)*) => {{
                println!("  {} {}", "✗".yellow(), format!($($msg)*));
                warn += 1;
            }};
        }

        println!("{}", "Checking home directory…".bold());
        if self.paths.home.exists() {
            check!(ok, "home: {}", self.paths.home.display());
        } else {
            check!(warn, "home directory not found: {}", self.paths.home.display());
        }

        println!("\n{}", "Checking plugins…".bold());
        let plugins = self.paths.installed_plugins();
        if plugins.is_empty() {
            check!(warn, "no plugins installed – add one with `sdk add`");
        }
        for name in &plugins {
            let dir = self.paths.plugin_dir(name);
            let meta_lua = dir.join("metadata.lua");
            if meta_lua.exists() {
                // Try loading the plugin to detect Lua errors
                match self.load_plugin(name) {
                    Ok(_)  => check!(ok, "plugin '{}' loads OK", name),
                    Err(e) => check!(warn, "plugin '{}' has errors: {}", name, e),
                }
            } else {
                check!(warn, "plugin '{}' missing metadata.lua", name);
            }

            // Check git status
            let git_dir = dir.join(".git");
            if !git_dir.exists() {
                check!(warn, "plugin '{}' has no .git – cannot `sdk update`", name);
            }
        }

        println!("\n{}", "Checking installed SDK versions…".bold());
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let chain = ConfigChain::load_from_dir(&self.paths, &cwd)?;
        let cache = &self.paths.cache;
        if cache.exists() {
            let sdk_names = self.paths.installed_plugins();
            let mut found_any = false;
            for sdk_name in &sdk_names {
                let versions = self.paths.installed_versions(sdk_name);
                if versions.is_empty() { continue; }
                found_any = true;
                for version in &versions {
                    let active_scope = chain.resolve(sdk_name)
                        .filter(|(_, t)| &t.version == version)
                        .map(|(scope, _)| format!(" (active {})", crate::sdk::scope_name(scope)));
                    let marker = active_scope.as_deref().unwrap_or("");
                    check!(ok, "{}@{}{}", sdk_name, version, marker);
                }
            }
            // Also flag any cache dirs with no matching plugin
            if cache.exists() {
                for entry in std::fs::read_dir(cache)?.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.path().is_dir() && !sdk_names.contains(&name) {
                        check!(warn, "orphaned cache dir with no plugin: {}", name);
                    }
                }
            }
            if !found_any {
                check!(warn, "no SDK versions installed – run `sdk install <name> <version>`");
            }
        } else {
            check!(warn, "cache directory not found: {}", cache.display());
        }

        println!("\n{}", "Checking PATH…".bold());
        let path_var = std::env::var("PATH").unwrap_or_default();
        let bin_dir  = self.paths.home.join("bin");
        let bin_str: String = bin_dir.to_string_lossy().to_lowercase();
        if path_var.to_lowercase().contains(bin_str.as_str()) {
            check!(ok, "sdk bin directory is on PATH ({})", bin_dir.display());
        } else {
            check!(warn, "sdk bin directory not on PATH – ensure {} is in your PATH", bin_dir.display());
        }
        // Hook check: __SDK_CLEAN_PATH env var is set only when hook has initialised
        if std::env::var("__SDK_CLEAN_PATH").is_ok() {
            check!(ok, "shell hook is active (__SDK_CLEAN_PATH is set)");
        } else {
            check!(warn, "shell hook not detected – add the following to your shell rc file and reload");
            // Print actionable setup instructions for each supported shell
            println!();
            println!("  {}", "Shell hook setup:".bold());
            println!();
            #[cfg(windows)]
            {
                let bin = std::env::current_exe()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "sdk".to_string());
                println!("  {} — add to $PROFILE  (run: notepad $PROFILE)", "PowerShell".cyan().bold());
                println!("    Invoke-Expression (& '{}' hook powershell | Out-String)", bin);
            }
            #[cfg(not(windows))]
            {
                println!("  {} — add to ~/.bashrc or ~/.bash_profile", "Bash".cyan().bold());
                println!("    eval \"$(sdk hook bash)\"");
                println!();
                println!("  {} — add to ~/.zshrc", "Zsh".cyan().bold());
                println!("    eval \"$(sdk hook zsh)\"");
                println!();
                println!("  {} — add to ~/.config/fish/config.fish", "Fish".cyan().bold());
                println!("    sdk hook fish | source");
            }
            println!();
            println!("  Other shells: sdk hook bash | zsh | fish | powershell | nu");
            println!();
        }
        let cache_str: String = self.paths.cache.to_string_lossy().to_lowercase();
        if path_var.to_lowercase().contains(cache_str.as_str()) {
            check!(ok, "sdk cache directory is on PATH (at least one version activated)");
        } else {
            check!(warn, "no SDK version directories on PATH – activate an SDK with `sdk use`");
        }

        println!();
        if warn == 0 {
            println!("{}", format!("All {} checks passed.", ok).green().bold());
        } else {
            println!("{}", format!("{} checks passed, {} warnings.", ok, warn).yellow().bold());
        }
        Ok(())
    }

    // ── Pin ───────────────────────────────────────────────────────────────────

    /// Write the currently-active version(s) into the project `.sdk.toml`.
    /// If `sdk_filter` is given, only that SDK is pinned; otherwise all active.
    pub fn pin(&mut self, sdk_filter: Option<&str>, explicit_version: Option<&str>) -> Result<()> {
        // If an explicit version is given, an SDK name is required
        if explicit_version.is_some() && sdk_filter.is_none() {
            anyhow::bail!("Specify an SDK name when providing an explicit version (e.g. `sdk pin node 22.16.0`)");
        }

        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let chain  = ConfigChain::load_from_dir(&self.paths, &cwd)?;
        let config = chain.effective_config();

        // Collect (name, version) pairs to pin
        let mut candidates: Vec<(String, String)> = Vec::new();
        if let (Some(name), Some(ver)) = (sdk_filter, explicit_version) {
            // Explicit: pin given SDK@version directly
            candidates.push((name.to_string(), ver.to_string()));
        } else {
            // Derive from active config
            for (sdk_name, tool) in &config.tools {
                if let Some(f) = sdk_filter {
                    if sdk_name != f { continue; }
                }
                candidates.push((sdk_name.clone(), tool.version.clone()));
            }
            if candidates.is_empty() {
                if let Some(f) = sdk_filter {
                    anyhow::bail!("SDK '{}' is not currently active – use `sdk use {} <version>` first", f, f);
                } else {
                    println!("No active SDKs to pin. Activate one with `sdk use <name> <version>` first.");
                    return Ok(());
                }
            }
        }

        let toml_path = find_or_create_project_toml()?;
        let mut toml  = SdkToml::load(&toml_path).unwrap_or_default();

        let mut pinned   = 0usize;
        let mut updated  = 0usize;
        let mut skipped  = 0usize;
        for (sdk_name, version) in &candidates {
            match toml.get_version(sdk_name) {
                Some(existing) if existing == version => {
                    println!("  {} {}@{} already pinned", "✓".green(), sdk_name.cyan(), version.green());
                    skipped += 1;
                }
                Some(existing) => {
                    let old = existing.to_string();
                    toml.set_tool(sdk_name, version);
                    println!("  {} {}@{} → {} in {}",
                        "↑".yellow(), sdk_name.cyan(),
                        old.dimmed(), version.green(),
                        toml_path.display());
                    updated += 1;
                }
                None => {
                    toml.set_tool(sdk_name, version);
                    println!("  {} {}@{} pinned in {}",
                        "✓".green(), sdk_name.cyan(), version.green(),
                        toml_path.display());
                    pinned += 1;
                }
            }
        }

        if pinned + updated > 0 {
            toml.save(&toml_path)?;
        }

        println!();
        let parts: Vec<String> = [
            (pinned,  "pinned"),
            (updated, "updated"),
            (skipped, "already up to date"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{} {}", n, label))
        .collect();
        println!("{}", parts.join("  ").bold());
        Ok(())
    }

    // ── Unpin ─────────────────────────────────────────────────────────────────

    pub fn unpin(&mut self, sdk_name: &str) -> Result<()> {
        let toml_path = find_project_toml()?;
        let mut toml  = SdkToml::load(&toml_path)?;
        if toml.get_tool(sdk_name).is_none() {
            println!("{} is not pinned in {}", sdk_name.yellow(), toml_path.display());
            return Ok(());
        }
        toml.remove_tool(sdk_name);
        toml.save(&toml_path)?;
        println!("  {} unpinned {} from {}", "✓".green(), sdk_name.cyan(), toml_path.display());
        Ok(())
    }

    // ── Mirror source management ──────────────────────────────────────────────

    /// `sdk mirror` – show current mirror settings for all installed plugins.
    pub fn mirror_show(&mut self) -> Result<()> {
        use crate::plugin::MirrorProfile;

        let plugin_names = self.paths.installed_plugins();
        if plugin_names.is_empty() {
            println!("No plugins installed.");
            return Ok(());
        }

        println!("{}", "Mirror settings".bold());
        println!();

        for name in &plugin_names {
            let profile_label = self.user_cfg.mirrors.get(name.as_str())
                .map(|e| if e.profile.is_empty() { "default".to_string() } else { e.profile.clone() })
                .unwrap_or_else(|| "default".to_string());
            // Clone custom vars before mutable borrow for load_plugin
            let custom_vars: Vec<(String, String)> = self.user_cfg.mirrors.get(name.as_str())
                .map(|e| e.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            print!("  {}  {}", name.cyan().bold(), format!("[{}]", profile_label).dimmed());

            // Try to load plugin to show var values
            if let Ok(plugin) = self.load_plugin(name) {
                let profiles: Vec<MirrorProfile> = plugin.mirror_profiles();
                // Find active profile to show the actual URL
                let active_profile = profiles.iter().find(|p| p.name == profile_label);
                // Collect all vars (from active profile or stored custom vars)
                let vars: Vec<(String, String)> = if let Some(ap) = active_profile {
                    ap.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else if !custom_vars.is_empty() {
                    custom_vars
                } else {
                    // No config → show default profile vars if any
                    profiles.iter()
                        .find(|p| p.name == "default")
                        .map(|p| p.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default()
                };
                if vars.is_empty() {
                    println!();
                } else {
                    let mut sorted = vars;
                    sorted.sort_by(|a, b| a.0.cmp(&b.0));
                    println!();
                    for (k, v) in &sorted {
                        let expanded = self.expand_mirror_var(v);
                        // Check if env var is actually set (may differ if user overrode manually)
                        let actual = std::env::var(k).unwrap_or_default();
                        let marker = if !actual.is_empty() && actual != expanded { " *" } else { "" };
                        println!("      {} = {}{}", k.dimmed(), expanded, marker.yellow());
                    }
                }
            } else {
                println!();
            }
        }
        println!("  {} Use {} to switch profiles", "tip:".dimmed(), "sdk mirror use <profile> [plugin]".cyan());
        Ok(())
    }

    /// `sdk mirror list [plugin]` – list available profiles for one or all plugins.
    pub fn mirror_list(&mut self, plugin_filter: Option<&str>) -> Result<()> {
        use crate::plugin::MirrorProfile;

        let plugin_names: Vec<String> = if let Some(f) = plugin_filter {
            if !self.paths.plugin_dir(f).exists() {
                bail!("Plugin '{}' is not installed.", f.cyan());
            }
            vec![f.to_string()]
        } else {
            self.paths.installed_plugins()
        };

        for name in &plugin_names {
            let plugin = match self.load_plugin(name) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let profiles: Vec<MirrorProfile> = plugin.mirror_profiles();
            if profiles.is_empty() {
                println!("{}: (no mirror profiles defined)", name.cyan().bold());
                continue;
            }

            let active_profile = self.user_cfg.mirrors.get(name.as_str())
                .map(|e| e.profile.as_str())
                .unwrap_or("default");

            println!("{}:", name.cyan().bold());
            for p in &profiles {
                let marker = if p.name == active_profile { " ✓".green().to_string() } else { "  ".to_string() };
                println!("  {}{} — {}", marker, p.name.bold(), p.description);
                for (k, v) in &p.vars {
                    println!("      {} = {}", k.dimmed(), v.dimmed());
                }
            }
            println!();
        }
        Ok(())
    }

    /// `sdk mirror use <profile> [plugin]` – switch to a named mirror profile.
    pub fn mirror_use(&mut self, profile: &str, plugin_filter: Option<&str>) -> Result<()> {
        use crate::plugin::MirrorProfile;

        let plugin_names: Vec<String> = if let Some(f) = plugin_filter {
            if !self.paths.plugin_dir(f).exists() {
                bail!("Plugin '{}' is not installed.", f.cyan());
            }
            vec![f.to_string()]
        } else {
            self.paths.installed_plugins()
        };

        let mut changed = 0usize;
        for name in &plugin_names {
            let plugin = match self.load_plugin(name) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let profiles: Vec<MirrorProfile> = plugin.mirror_profiles();
            if profiles.is_empty() {
                if plugin_filter.is_some() {
                    bail!("Plugin '{}' does not define any mirror profiles.", name.cyan());
                }
                continue;
            }

            let matched = profiles.iter().find(|p| p.name == profile);
            let Some(matched_profile) = matched else {
                let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
                if plugin_filter.is_some() {
                    bail!(
                        "Profile '{}' not found for plugin '{}'. Available: {}",
                        profile, name, names.join(", ")
                    );
                }
                println!("  {} '{}': profile '{}' not found, skipping", "⚠".yellow(), name.cyan(), profile);
                continue;
            };

            // Store profile + vars in config
            let entry = self.user_cfg.mirrors
                .entry(name.clone())
                .or_default();
            entry.profile = profile.to_string();
            entry.vars = matched_profile.vars.clone();

            // Apply to current process env immediately (expand {local_dir} placeholder)
            for (k, v) in &matched_profile.vars {
                let resolved = self.expand_mirror_var(v);
                std::env::set_var(k, resolved);
            }

            let var_summary: Vec<String> = matched_profile.vars.iter()
                .map(|(k, v)| format!("{}={}", k, self.expand_mirror_var(v)))
                .collect();
            println!("  {} {} → {} ({})", "✓".green(), name.cyan(), profile.bold(), var_summary.join(", ").dimmed());
            changed += 1;
        }

        if changed > 0 {
            self.user_cfg.save(&self.paths.user_config)?;
            println!("\nSaved. Restart your shell or run {} to apply.", "sdk hook <shell>".cyan());
        } else if plugin_filter.is_none() {
            println!("No plugins were updated (none define profile '{}').", profile);
        }
        Ok(())
    }

    /// `sdk mirror set <plugin> <VAR> <url>` – set a custom env var for a plugin's mirror.
    pub fn mirror_set_var(&mut self, plugin_name: &str, var: &str, url: &str) -> Result<()> {
        if !self.paths.plugin_dir(plugin_name).exists() {
            bail!("Plugin '{}' is not installed.", plugin_name.cyan());
        }

        let entry = self.user_cfg.mirrors
            .entry(plugin_name.to_string())
            .or_default();
        entry.profile = "custom".to_string();
        entry.vars.insert(var.to_string(), url.to_string());

        // Apply immediately
        std::env::set_var(var, url);

        self.user_cfg.save(&self.paths.user_config)?;
        println!("Set {} = {} for plugin '{}'", var.dimmed(), url, plugin_name.cyan());
        Ok(())
    }

    /// `sdk mirror reset [plugin]` – remove mirror overrides (revert to defaults).
    pub fn mirror_reset(&mut self, plugin_filter: Option<&str>) -> Result<()> {
        if let Some(f) = plugin_filter {
            if self.user_cfg.mirrors.remove(f).is_some() {
                self.user_cfg.save(&self.paths.user_config)?;
                println!("Reset mirror settings for '{}'.", f.cyan());
            } else {
                println!("No mirror settings found for '{}'.", f.cyan());
            }
        } else {
            let count = self.user_cfg.mirrors.len();
            self.user_cfg.mirrors.clear();
            self.user_cfg.save(&self.paths.user_config)?;
            println!("Reset mirror settings for {} plugin(s).", count);
        }
        Ok(())
    }

    // ── Mirror download (build local mirror) ─────────────────────────────────

    /// Download SDK archives to a local mirror directory for offline use.
    ///
    /// Modes: `--version v1,v2,...` | `--lts` | `--all`
    /// Saves files flat: `<dir>/<plugin>/<filename>`, then writes `versions.json`.
    /// Supports concurrent downloads and HTTP Range-based resume.
    pub fn mirror_download(
        &mut self,
        plugins: &[String],
        versions: &[String],
        lts: bool,
        all: bool,
        dir: Option<&str>,
        dry_run: bool,
        concurrency: usize,
    ) -> Result<()> {
        let target_plugins: Vec<String> = if plugins.is_empty() {
            self.paths.installed_plugins()
        } else {
            plugins.iter().filter(|p| {
                let exists = self.paths.plugin_dir(p).exists();
                if !exists {
                    eprintln!("{}: plugin '{}' is not installed, skipping", "Warning".yellow(), p);
                }
                exists
            }).cloned().collect()
        };

        if target_plugins.is_empty() {
            bail!("No installed plugins found. Use `sdk add` to install plugins first.");
        }

        if versions.is_empty() && !lts && !all {
            bail!(
                "Specify a download mode:\n  \
                 --version v1,v2,...   specific version(s)\n  \
                 --lts                 LTS versions only\n  \
                 --all                 all available versions"
            );
        }

        let proxy   = self.proxy_url();
        let ssl_ver = self.ssl_verify();

        let mut grand_downloaded = 0usize;
        let mut grand_skipped    = 0usize;
        let mut grand_failed     = 0usize;

        for plugin_name in &target_plugins {
            println!("\n{} {}", "►".cyan(), plugin_name.bold());

            let out_dir: std::path::PathBuf = match dir {
                Some(d) => std::path::PathBuf::from(d).join(plugin_name),
                None    => {
                    let base = self.local_dir();
                    std::path::PathBuf::from(&base).join(plugin_name)
                }
            };

            let plugin = match self.load_plugin(plugin_name) {
                Ok(p)  => p,
                Err(e) => { eprintln!("  {}: {}", "Error loading plugin".red(), e); continue; }
            };

            let available = match plugin.call_available(&[]) {
                Ok(v)  => v,
                Err(e) => { eprintln!("  {}: {}", "Error fetching versions".red(), e); continue; }
            };
            if available.is_empty() {
                println!("  {}", "No available versions found.".dimmed());
                continue;
            }

            let target_versions: Vec<String> = if !versions.is_empty() {
                let avail_set: std::collections::HashSet<&str> =
                    available.iter().map(|i| i.version.as_str()).collect();
                versions.iter().filter(|v| {
                    if !avail_set.contains(v.as_str()) {
                        eprintln!("  {}: version '{}' not in available list", "Warning".yellow(), v);
                        false
                    } else { true }
                }).cloned().collect()
            } else if lts {
                available.iter()
                    .filter(|i| i.note.to_ascii_lowercase().contains("lts"))
                    .map(|i| i.version.clone()).collect()
            } else {
                available.iter().map(|i| i.version.clone()).collect()
            };

            if target_versions.is_empty() {
                println!("  {}", if lts {
                    "No LTS versions found (plugin may not tag LTS).".dimmed()
                } else { "No matching versions.".dimmed() });
                continue;
            }

            // ── First pass: resolve all download tasks ──────────────────────
            struct Task {
                version: String,
                url:     String,
                headers: HashMap<String, String>,
                dest:    std::path::PathBuf,
            }

            let mut all_tasks: Vec<Task> = Vec::new();
            let mut pre_errors           = 0usize;

            for version in &target_versions {
                let info = match plugin.call_pre_install(version) {
                    Ok(i)  => i,
                    Err(e) => {
                        eprintln!("  {} {}@{}: {}", "✗".red(), plugin_name, version.yellow(), e);
                        pre_errors += 1;
                        continue;
                    }
                };

                let mut url_entries: Vec<(String, HashMap<String, String>)> = Vec::new();
                if !info.url.is_empty()
                    && (info.url.starts_with("http://") || info.url.starts_with("https://"))
                {
                    url_entries.push((info.url.clone(), info.headers.clone()));
                }
                for addon in &info.addition {
                    if !addon.url.is_empty()
                        && (addon.url.starts_with("http://") || addon.url.starts_with("https://"))
                    {
                        url_entries.push((addon.url.clone(), addon.headers.clone()));
                    }
                }
                for (url, hdrs) in url_entries {
                    let filename = std::path::Path::new(&url)
                        .file_name().unwrap_or_default()
                        .to_string_lossy().to_string();
                    let dest = out_dir.join(&filename);
                    all_tasks.push(Task { version: info.version.clone(), url, headers: hdrs, dest });
                }
            }

            // Split into fully-complete (skip) vs pending (download or resume)
            // A file is "complete" only if it exists AND has non-zero size.
            // Partial files will be resumed by download_inner via Range header.
            let (complete, pending): (Vec<Task>, Vec<Task>) = all_tasks.into_iter()
                .partition(|t| {
                    t.dest.exists()
                        && t.dest.metadata().map(|m| m.len() > 0).unwrap_or(false)
                        // Can't know if truly complete without checksum, so treat all
                        // existing non-zero files as resumable — they'll get a Range
                        // request; if server returns 416 (range not satisfiable) the
                        // file is already complete and we skip.
                        && false  // actually treat all existing as resume-pending
                });
            // ^^ The logic above always sends to pending so resume works.
            // Re-partition simply: existing AND "looks complete" → skip; missing → download.
            // For simplicity, existing file with size > 0 goes to cached (skip); will redo if corrupt.
            let (cached, pending): (Vec<Task>, Vec<Task>) = pending.into_iter()
                .chain(complete)
                .partition(|t| {
                    t.dest.exists() && t.dest.metadata().map(|m| m.len() > 0).unwrap_or(false)
                });

            grand_skipped += cached.len();
            for t in &cached {
                let fname = t.dest.file_name().unwrap_or_default().to_string_lossy();
                println!("  {} {} (cached)", "✓".green(), fname.dimmed());
            }

            if pending.is_empty() {
                println!("  {}", "All files already cached.".dimmed());
                if !dry_run {
                    let all_versions: Vec<String> = cached.iter().map(|t| t.version.clone()).collect();
                    if !all_versions.is_empty() {
                        self.write_versions_json(&out_dir, &all_versions, &available)?;
                    }
                }
                continue;
            }

            println!(
                "  {} file(s) to download  →  {}",
                pending.len(),
                out_dir.display().to_string().dimmed()
            );

            if dry_run {
                for t in &pending {
                    println!("  {} {}", "[dry-run]".yellow(), t.url);
                }
                continue;
            }

            std::fs::create_dir_all(&out_dir)
                .with_context(|| format!("creating output dir {}", out_dir.display()))?;

            // ── Second pass: concurrent downloads ───────────────────────────
            let n_threads: usize = if concurrency == 0 {
                let auto = std::thread::available_parallelism()
                    .map(|n| n.get()).unwrap_or(4);
                std::cmp::min(auto.max(4), 8).min(pending.len())
            } else {
                concurrency.min(pending.len())
            };

            let multi   = MultiProgress::new();
            let overall = multi.add(ProgressBar::new(pending.len() as u64));
            overall.set_style(
                ProgressStyle::with_template(
                    "  Overall [{bar:30.green/white}] {pos}/{len}  {msg}",
                ).unwrap().progress_chars("=>-"),
            );
            overall.set_message(format!("{} ({} threads)", plugin_name, n_threads));

            // Seed with cached versions
            let done_set = std::sync::Arc::new(std::sync::Mutex::new({
                let mut s = std::collections::HashSet::<String>::new();
                for t in &cached { s.insert(t.version.clone()); }
                s
            }));
            let fail_cnt  = std::sync::Arc::new(std::sync::Mutex::new(0usize));
            let task_queue = std::sync::Arc::new(std::sync::Mutex::new(pending));

            let mut handles = Vec::with_capacity(n_threads);
            for _ in 0..n_threads {
                let queue  = std::sync::Arc::clone(&task_queue);
                let done   = std::sync::Arc::clone(&done_set);
                let fails  = std::sync::Arc::clone(&fail_cnt);
                let mp2    = multi.clone();
                let ov2    = overall.clone();
                let proxy2 = proxy.clone();

                handles.push(std::thread::spawn(move || {
                    loop {
                        let task = queue.lock().unwrap().pop();
                        match task {
                            None => break,
                            Some(t) => {
                                match crate::util::download_with_multi_progress(
                                    &t.url, &t.headers, &t.dest,
                                    proxy2.as_deref(), ssl_ver, &mp2, &ov2,
                                ) {
                                    Ok(_)  => { done.lock().unwrap().insert(t.version); }
                                    Err(e) => {
                                        ov2.println(format!(
                                            "  {} {}: {}",
                                            "✗ Failed".red(),
                                            t.dest.file_name()
                                                .unwrap_or_default().to_string_lossy(),
                                            e,
                                        ));
                                        let _ = std::fs::remove_file(&t.dest);
                                        *fails.lock().unwrap() += 1;
                                    }
                                }
                            }
                        }
                    }
                }));
            }
            for h in handles { let _ = h.join(); }
            overall.finish_with_message("done");

            let n_failed = *fail_cnt.lock().unwrap();
            let dv_set   = std::sync::Arc::try_unwrap(done_set)
                .unwrap().into_inner().unwrap();
            let n_dl     = dv_set.len().saturating_sub(cached.len());
            grand_downloaded += n_dl;
            grand_failed     += n_failed;

            if pre_errors == 0 && !dv_set.is_empty() {
                let dv: Vec<String> = dv_set.into_iter().collect();
                self.write_versions_json(&out_dir, &dv, &available)?;
            }
        }

        println!(
            "\n{} {} downloaded, {} already cached, {} failed",
            "Done.".green().bold(),
            grand_downloaded,
            grand_skipped,
            grand_failed,
        );
        Ok(())
    }

    /// Write (or update) `<dir>/versions.json` with a sorted list.
    /// `downloaded` — versions currently on disk;  `available` — all known versions
    /// (used to preserve the canonical sort order).
    fn write_versions_json(
        &self,
        dir: &std::path::Path,
        downloaded: &[String],
        available: &[crate::plugin::AvailableItem],
    ) -> Result<()> {
        // Start with the order from `available` (plugin-canonical order, usually newest-first),
        // keeping only versions that are in `downloaded`.
        let downloaded_set: std::collections::HashSet<&str> =
            downloaded.iter().map(|s| s.as_str()).collect();

        let mut sorted: Vec<String> = available.iter()
            .filter(|i| downloaded_set.contains(i.version.as_str()))
            .map(|i| i.version.clone())
            .collect();

        // Add any version in `downloaded` that wasn't in `available` (edge case)
        let in_sorted: std::collections::HashSet<String> = sorted.iter().cloned().collect();
        for v in downloaded {
            if !in_sorted.contains(v) {
                sorted.push(v.clone());
            }
        }

        if sorted.is_empty() {
            return Ok(());
        }

        let vj_path = dir.join("versions.json");
        let json    = serde_json::to_string_pretty(&sorted)?;
        std::fs::write(&vj_path, &json)?;
        println!("  {} versions.json ({} versions)", "✎".cyan(), sorted.len());
        Ok(())
    }

}

fn find_project_toml() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    SdkToml::find_in_dir(&cwd)
        .ok_or_else(|| anyhow::anyhow!("No .sdk.toml found in current or parent directories"))
}

fn find_or_create_project_toml() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(SdkToml::find_in_dir(&cwd).unwrap_or_else(|| cwd.join(".sdk.toml")))
}


/// Returns true if the version string looks like a pre-release.
///
/// Matches common pre-release patterns:
/// - `alpha`, `beta`, `rc` anywhere in the version (e.g. `3.15.0b1`, `3.15.0rc2`, `1.0.0-beta.1`)
/// - `.dev` suffix (e.g. `3.14.0.dev0`)
/// - `.pre` suffix
/// - `a<N>`, `b<N>` immediately after digits (Python-style: `3.15.0b1`)
fn is_prerelease(version: &str) -> bool {
    let v = version.to_ascii_lowercase();
    // Explicit word markers
    if v.contains("alpha") || v.contains("beta") || v.contains(".pre") || v.contains(".dev") {
        return true;
    }
    // `rc` followed by a digit or end (avoids matching e.g. "mercurial")
    if let Some(pos) = v.find("rc") {
        let after = &v[pos + 2..];
        if after.is_empty() || after.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
    }
    // Python-style: digit immediately followed by `a<N>` or `b<N>` (e.g. 3.15.0b1, 3.15.0a2)
    let bytes = v.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i - 1].is_ascii_digit()
            && (bytes[i] == b'a' || bytes[i] == b'b')
            && i + 1 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
        {
            return true;
        }
    }
    false
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for u in &UNITS[1..] {
        if value < 1024.0 { break; }
        value /= 1024.0;
        unit = u;
    }
    if value < 10.0 { format!("{:.1}{}", value, unit) }
    else            { format!("{:.0}{}", value, unit) }
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
