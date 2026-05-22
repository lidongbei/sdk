use std::{collections::HashMap, sync::Arc};

use anyhow::{bail, Context, Result};
use colored::Colorize;

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

    /// Install a plugin from a registry path or URL.
    pub fn add_plugin(&self, name: &str, source: &str) -> Result<()> {
        let plugin_dir = self.paths.plugin_dir(name);
        if plugin_dir.exists() {
            println!("Plugin '{}' already exists.", name.cyan());
            return Ok(());
        }
        println!("Adding plugin '{}' from {}...", name.cyan(), source.blue());

        // Git clone
        let status = std::process::Command::new("git")
            .args(["clone", "--depth=1", source, plugin_dir.to_str().unwrap_or("")])
            .status()
            .context("git clone")?;
        if !status.success() {
            bail!("Failed to clone plugin from {}", source);
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

    // ── Install / Uninstall ───────────────────────────────────────────────────

    pub fn install(&mut self, sdk_name: &str, version: &str) -> Result<()> {
        let plugin = self.load_plugin(sdk_name)?;
        let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
        sdk.install(version)?;
        Ok(())
    }

    pub fn uninstall(&mut self, sdk_name: &str, version: &str) -> Result<()> {
        let plugin = self.load_plugin(sdk_name)?;
        let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());

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

    // ── Use / Unuse ───────────────────────────────────────────────────────────

    /// Write `sdk@version` to `.sdk.toml` (project or global, no symlinks).
    pub fn use_sdk(
        &mut self,
        sdk_name: &str,
        version: &str,
        scope: Scope,
    ) -> Result<()> {
        let plugin  = self.load_plugin(sdk_name)?;
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
                    let sdk    = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
            let sdk     = Sdk::new(name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
            let chain   = ConfigChain::load(&self.paths)?;
            let current = chain.resolve_version(name);

            let versions = sdk.installed_versions();
            if versions.is_empty() {
                println!("No versions of {} installed.", name.cyan());
            } else {
                println!("Installed versions of {}:", name.cyan());
                for v in &versions {
                    let marker = if current.as_deref() == Some(v.as_str()) { "→ " } else { "  " };
                    println!("  {}{}", marker.green(), v);
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
                            println!("  {}{}", marker.green(), v);
                        }
                    }
                }
            }
        }
        Ok(())
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
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
        let sdk     = Sdk::new(sdk_name.to_string(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());
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
    pub fn upgrade(&mut self, sdk_filter: Option<&str>, auto: bool) -> Result<()> {
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
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());

            print!("  {} (current: {}) checking…", sdk_name.cyan(), current.yellow());
            let latest = sdk.available(&[])
                .ok()
                .and_then(|items| items.into_iter().next().map(|i| i.version));

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

    // ── Search registry ───────────────────────────────────────────────────────

    pub fn search(&self, query: Option<&str>) -> Result<()> {
        const REGISTRY: &str = "https://version-fox.github.io/vfox-plugins/index.json";

        let client = crate::util::build_http_client(self.proxy_url().as_deref(), self.ssl_verify())
            .context("building HTTP client")?;
        let resp = client.get(REGISTRY).send()
            .context("fetching plugin registry")?;
        if !resp.status().is_success() {
            bail!("registry returned HTTP {}", resp.status());
        }

        #[derive(serde::Deserialize)]
        struct Item {
            name:     String,
            desc:     String,
            homepage: String,
        }

        let items: Vec<Item> = resp.json().context("parsing registry index")?;

        let filtered: Vec<&Item> = if let Some(q) = query {
            let q = q.to_lowercase();
            items.iter().filter(|i| {
                i.name.to_lowercase().contains(&q) || i.desc.to_lowercase().contains(&q)
            }).collect()
        } else {
            items.iter().collect()
        };

        if filtered.is_empty() {
            println!("No plugins found{}.", query.map(|q| format!(" matching '{}'", q)).unwrap_or_default());
            return Ok(());
        }

        // Column widths — name capped at 25, description at 55 for legibility
        const MAX_NAME_W: usize = 25;
        const MAX_DESC_W: usize = 55;
        let name_w = filtered.iter().map(|i| i.name.len()).max().unwrap_or(4).clamp(4, MAX_NAME_W);
        let desc_w = filtered.iter().map(|i| i.desc.len()).max().unwrap_or(11).clamp(11, MAX_DESC_W);

        println!(
            "{:<name_w$}  {:<desc_w$}  {}",
            "NAME".bold(),
            "DESCRIPTION".bold(),
            "HOMEPAGE".bold(),
            name_w = name_w,
            desc_w = desc_w,
        );
        println!("{}", "─".repeat(name_w + desc_w + 42));

        for item in &filtered {
            let installed = self.paths.plugin_dir(&item.name).exists();
            // Truncate long values to fit columns
            let name_display = truncate(&item.name, MAX_NAME_W);
            let desc_display = truncate(&item.desc, MAX_DESC_W);
            let suffix = if installed { format!(" {}", "(installed)".dimmed()) } else { String::new() };
            println!(
                "{:<name_w$}{}  {:<desc_w$}  {}",
                name_display,
                suffix,
                desc_display,
                item.homepage.dimmed(),
                name_w = name_w,
                desc_w = desc_w,
            );
        }

        println!("\nAdd a plugin:  {}", "sdk add <name> <homepage-url>".cyan());
        Ok(())
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
            check!(warn, "shell hook not detected – add `sdk hook <shell>` to your shell rc and reload");
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
    pub fn pin(&mut self, sdk_filter: Option<&str>) -> Result<()> {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let chain  = ConfigChain::load_from_dir(&self.paths, &cwd)?;
        let config = chain.effective_config();

        if config.tools.is_empty() {
            println!("No active SDKs to pin.");
            return Ok(());
        }

        let toml_path = find_or_create_project_toml()?;
        let mut toml  = SdkToml::load(&toml_path).unwrap_or_default();

        let mut pinned = 0usize;
        for (sdk_name, tool) in &config.tools {
            if let Some(f) = sdk_filter {
                if sdk_name != f { continue; }
            }
            let version = &tool.version;
            toml.set_tool(sdk_name, version);
            println!("Pinned {}@{} in {}", sdk_name.cyan(), version.green(), toml_path.display());
            pinned += 1;
        }

        if pinned == 0 {
            if let Some(f) = sdk_filter {
                println!("SDK '{}' is not currently active.", f.yellow());
            }
            return Ok(());
        }

        toml.save(&toml_path)?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn find_project_toml() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    SdkToml::find_in_dir(&cwd)
        .ok_or_else(|| anyhow::anyhow!("No .sdk.toml found in current or parent directories"))
}

fn find_or_create_project_toml() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(SdkToml::find_in_dir(&cwd).unwrap_or_else(|| cwd.join(".sdk.toml")))
}

/// Truncate a string to `max` chars, appending `…` if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
