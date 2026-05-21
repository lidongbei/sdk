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
        let meta_path  = plugin_dir.join("metadata.lua");
        if !meta_path.exists() {
            bail!("Plugin '{}' not found.", sdk_name);
        }
    let _plugin = self.load_plugin(sdk_name)?.clone();
        println!("Plugin: {}", sdk_name.cyan());
        println!("Path:   {}", plugin_dir.display());
        let installed = self.paths.installed_versions(sdk_name);
        println!("Installed versions: {}", installed.len());
        for v in &installed {
            println!("  {}", v.green());
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
        let chain   = ConfigChain::load(&self.paths)?;
        let global  = SdkToml::load(&self.paths.global_toml).unwrap_or_default();
        let config  = chain.effective_config();

        if config.tools.is_empty() {
            println!("No active SDKs to check.");
            return Ok(());
        }

        let mut any = false;
        for (sdk_name, tool) in &config.tools {
            if let Some(f) = sdk_filter {
                if sdk_name != f { continue; }
            }
            any = true;
            let current = &tool.version;
            let plugin  = match self.load_plugin(sdk_name) {
                Ok(p)  => p,
                Err(e) => {
                    eprintln!("  Warning: {} – {}", sdk_name, e);
                    continue;
                }
            };
            let sdk = Sdk::new(sdk_name.clone(), plugin, &self.paths, self.proxy_url(), self.ssl_verify());

            print!("  {} (current: {}) … checking … ", sdk_name.cyan(), current.yellow());
            let latest = sdk.available(&[])
                .ok()
                .and_then(|items| items.into_iter().next().map(|i| i.version));

            match latest {
                None => println!("{}", "unavailable".dimmed()),
                Some(ref latest) if latest == current => println!("{}", "up to date".green()),
                Some(ref latest) => {
                    println!("newer: {}", latest.green());
                    if auto {
                        let is_global = global.get_version(sdk_name) == Some(current.as_str());
                        let scope = if is_global { Scope::Global } else { Scope::Project };
                        println!("  Upgrading {} → {} …", current.yellow(), latest.green());
                        if let Err(e) = self.use_sdk(sdk_name, latest, scope) {
                            eprintln!("  Error: {}", e);
                        }
                    }
                }
            }
        }
        if !any {
            if let Some(f) = sdk_filter {
                println!("SDK '{}' is not currently active.", f);
            }
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

        // Column widths
        let name_w = filtered.iter().map(|i| i.name.len()).max().unwrap_or(4).max(4);
        let desc_w = filtered.iter().map(|i| i.desc.len()).max().unwrap_or(11).max(11);

        println!(
            "{:<name_w$}  {:<desc_w$}  {}",
            "NAME".bold(),
            "DESCRIPTION".bold(),
            "HOMEPAGE".bold(),
            name_w = name_w,
            desc_w = desc_w,
        );
        println!("{}", "-".repeat(name_w + desc_w + 40));

        for item in &filtered {
            let installed = self.paths.plugin_dir(&item.name).exists();
            let name_str = if installed {
                format!("{} {}", item.name, "(installed)".dimmed())
            } else {
                item.name.clone()
            };
            println!(
                "{:<name_w$}  {:<desc_w$}  {}",
                name_str,
                item.desc,
                item.homepage.dimmed(),
                name_w = name_w,
                desc_w = desc_w,
            );
        }

        println!("\nAdd a plugin with: {}", "sdk add <name> <homepage>".cyan());
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
        let chain  = ConfigChain::load(&self.paths)?;
        let config = chain.effective_config();
        let cache  = &self.paths.cache;
        if cache.exists() {
            for sdk_entry in std::fs::read_dir(cache)?.flatten() {
                let sdk_name = sdk_entry.file_name().to_string_lossy().to_string();
                if !sdk_entry.path().is_dir() { continue; }
                for ver_entry in std::fs::read_dir(sdk_entry.path())?.flatten() {
                    if !ver_entry.path().is_dir() { continue; }
                    let version = ver_entry.file_name().to_string_lossy().to_string();
                    let is_active = config.tools.get(&sdk_name)
                        .map(|t| t.version == version)
                        .unwrap_or(false);
                    let marker = if is_active { " (active)" } else { "" };
                    check!(ok, "{}@{}{}", sdk_name, version, marker);
                }
            }
        } else {
            check!(warn, "cache directory not found: {}", cache.display());
        }

        println!("\n{}", "Checking PATH…".bold());
        let path_var = std::env::var("PATH").unwrap_or_default();
        // Check if cache dir appears in PATH at all (it should if shell hook is active)
        let cache_str = self.paths.cache.to_string_lossy().to_lowercase();
        let cache_str: &str = cache_str.as_ref();
        if path_var.to_lowercase().contains(cache_str) {
            check!(ok, "sdk cache directory is on PATH (shell hook active)");
        } else {
            check!(warn, "sdk cache directory not on PATH – run `sdk activate <shell>` and reload shell");
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
