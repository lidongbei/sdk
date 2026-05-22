use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;

// ═══════════════════════════════════════════════════════════════════════════════
// SdkToml – .sdk.toml format
// ═══════════════════════════════════════════════════════════════════════════════

/// Additional per-tool attributes stored alongside the version.
/// e.g. `java = { version = "21", vendor = "openjdk" }`
pub type ToolAttrs = BTreeMap<String, String>;

/// Configuration for a single tool.
#[derive(Debug, Clone)]
pub struct ToolConfig {
    pub version: String,
    pub attrs:   ToolAttrs,
}

impl ToolConfig {
    pub fn simple(version: impl Into<String>) -> Self {
        Self { version: version.into(), attrs: BTreeMap::new() }
    }
}

/// The `[tools]` section of `.sdk.toml`.
///
/// Supports two formats:
/// ```toml
/// nodejs = "20.0.0"
/// java   = { version = "21", vendor = "openjdk" }
/// ```
#[derive(Debug, Default, Clone)]
pub struct SdkToml {
    /// Path this config was loaded from (empty for in-memory configs).
    pub path:  Option<PathBuf>,
    pub tools: BTreeMap<String, ToolConfig>,
}

impl SdkToml {
    #[allow(dead_code)]
    pub fn new() -> Self { Self::default() }

    /// Load from a specific path.  Returns an empty config if the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self { path: Some(path.to_owned()), ..Default::default() });
        }        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut cfg = Self::parse(&text)
            .with_context(|| format!("parsing {}", path.display()))?;
        cfg.path = Some(path.to_owned());
        Ok(cfg)
    }

    /// Parse TOML text.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: toml::Value = toml::from_str(text)?;
        let mut tools = BTreeMap::new();

        if let Some(section) = raw.get("tools").and_then(|v| v.as_table()) {
            for (name, value) in section {
                let cfg = match value {
                    toml::Value::String(v) => ToolConfig::simple(v),
                    toml::Value::Table(t) => {
                        let version = t
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let mut attrs = BTreeMap::new();
                        for (k, v) in t {
                            if k != "version" {
                                attrs.insert(k.clone(), toml_value_to_string(v));
                            }
                        }
                        ToolConfig { version, attrs }
                    }
                    _ => continue,
                };
                tools.insert(name.clone(), cfg);
            }
        }
        Ok(Self { path: None, tools })
    }

    /// Serialize to TOML text (sorted keys for reproducibility).
    pub fn to_toml_string(&self) -> String {
        if self.tools.is_empty() {
            return "[tools]\n".to_string();
        }
        let mut lines = vec!["[tools]".to_string()];
        for (name, cfg) in &self.tools {
            lines.push(format!("{} = {}", name, cfg.inline_toml()));
        }
        lines.push(String::new()); // trailing newline
        lines.join("\n")
    }

    /// Save to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if self.tools.is_empty() && !path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml_string())
            .with_context(|| format!("writing {}", path.display()))
    }

    // ── Tool management ───────────────────────────────────────────────────────

    pub fn set_tool(&mut self, name: &str, version: &str) {
        self.tools.insert(name.to_string(), ToolConfig::simple(version));
    }

    #[allow(dead_code)]
    pub fn set_tool_with_attrs(&mut self, name: &str, version: &str, attrs: ToolAttrs) {
        self.tools.insert(
            name.to_string(),
            ToolConfig { version: version.to_string(), attrs },
        );
    }

    pub fn get_tool(&self, name: &str) -> Option<&ToolConfig> {
        self.tools.get(name)
    }

    pub fn get_version(&self, name: &str) -> Option<&str> {
        self.tools.get(name).map(|c| c.version.as_str())
    }

    pub fn remove_tool(&mut self, name: &str) {
        self.tools.remove(name);
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.tools.is_empty() }

    /// Look for `.sdk.toml` or `sdk.toml` directly in `dir`.
    pub fn find_in_dir(dir: &Path) -> Option<PathBuf> {
        let p = dir.join(".sdk.toml");
        if p.exists() { return Some(p); }
        let p = dir.join("sdk.toml");
        if p.exists() { return Some(p); }
        None
    }
}

impl ToolConfig {
    /// Produce the inline TOML value, e.g. `"21.5.0"` or `{version = "21", vendor = "openjdk"}`.
    pub fn inline_toml(&self) -> String {
        if self.attrs.is_empty() {
            format!("\"{}\"", self.version.replace('"', "\\\""))
        } else {
            let mut parts = vec![format!("version = \"{}\"", self.version)];
            for (k, v) in &self.attrs {
                parts.push(format!("{} = \"{}\"", k, v));
            }
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn toml_value_to_string(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// UserConfig – config.yaml (user preferences)
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct UseConfig {
    /// Default scope for `sdk use` when no explicit flag is given.
    /// Valid values: "session", "project", "global"
    pub default_scope: String,
}

impl Default for UseConfig {
    fn default() -> Self { Self { default_scope: "session".to_string() } }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct UserConfig {
    pub proxy:     ProxyConfig,
    pub cache:     CacheConfig,
    pub storage:   StorageConfig,
    pub gitignore: GitignoreConfig,
    pub registry:  RegistryConfig,
    #[serde(rename = "use")]
    pub use_cfg:   UseConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub url: String,
    /// When false, skip TLS certificate verification (useful behind SSL-inspecting proxies).
    #[serde(default = "default_true")]
    pub ssl_verify: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Duration in minutes for the `Available` hook cache (0 = disabled).
    pub available_ttl: u64,
}

impl Default for CacheConfig {
    fn default() -> Self { Self { available_ttl: 60 } }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StorageConfig {
    /// Custom storage path for SDK installs (empty = default cache dir).
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GitignoreConfig {
    pub enable: bool,
}

impl Default for GitignoreConfig {
    fn default() -> Self { Self { enable: true } }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RegistryConfig {
    /// URL of the plugin registry manifest.
    pub url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://version-fox.github.io/vfox-plugins/plugins.json".to_string(),
        }
    }
}


impl UserConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        serde_yaml::from_str(&text).context("parsing config.yaml")
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_yaml::to_string(self).context("serializing config")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Get a config value by dotted key (e.g. `proxy.url`).
    pub fn get_key(&self, key: &str) -> Option<String> {
        match key {
            "proxy.enable"          => Some(self.proxy.enable.to_string()),
            "proxy.url"             => Some(self.proxy.url.clone()),
            "proxy.ssl_verify"      => Some(self.proxy.ssl_verify.to_string()),
            "cache.available_ttl"   => Some(self.cache.available_ttl.to_string()),
            "storage.path"          => Some(self.storage.path.clone()),
            "gitignore.enable"      => Some(self.gitignore.enable.to_string()),
            "registry.url"          => Some(self.registry.url.clone()),
            "use.default_scope"     => Some(self.use_cfg.default_scope.clone()),
            _ => None,
        }
    }

    /// Set a config value by dotted key. Returns an error for unknown keys.
    pub fn set_key(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "proxy.enable" => {
                self.proxy.enable = value.parse::<bool>()
                    .with_context(|| format!("'{}' must be true/false", key))?;
            }
            "proxy.url" => {
                self.proxy.url = value.to_string();
            }
            "proxy.ssl_verify" => {
                self.proxy.ssl_verify = value.parse::<bool>()
                    .with_context(|| format!("'{}' must be true/false", key))?;
            }
            "cache.available_ttl" => {
                self.cache.available_ttl = value.parse::<u64>()
                    .with_context(|| format!("'{}' must be a non-negative integer (minutes)", key))?;
            }
            "storage.path" => {
                self.storage.path = value.to_string();
            }
            "gitignore.enable" => {
                self.gitignore.enable = value.parse::<bool>()
                    .with_context(|| format!("'{}' must be true/false", key))?;
            }
            "registry.url" => {
                self.registry.url = value.to_string();
            }
            "use.default_scope" => {
                match value {
                    "session" | "project" | "global" => {
                        self.use_cfg.default_scope = value.to_string();
                    }
                    _ => anyhow::bail!(
                        "'use.default_scope' must be one of: session, project, global"
                    ),
                }
            }
            _ => anyhow::bail!("Unknown config key '{}'. Valid keys:\n  proxy.enable  proxy.url  proxy.ssl_verify  cache.available_ttl  storage.path  gitignore.enable  registry.url  use.default_scope", key),
        }
        Ok(())
    }

    /// All known keys with their current values.
    pub fn all_pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("proxy.enable",        self.proxy.enable.to_string()),
            ("proxy.url",           self.proxy.url.clone()),
            ("proxy.ssl_verify",    self.proxy.ssl_verify.to_string()),
            ("cache.available_ttl", self.cache.available_ttl.to_string()),
            ("storage.path",        self.storage.path.clone()),
            ("gitignore.enable",    self.gitignore.enable.to_string()),
            ("registry.url",        self.registry.url.clone()),
            ("use.default_scope",   self.use_cfg.default_scope.clone()),
        ]
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ConfigChain – layered version resolution (global → project → session)
// ═══════════════════════════════════════════════════════════════════════════════

/// Priority order: Session > Project > Global (later entries win).
#[derive(Debug, Default)]
pub struct ConfigChain {
    layers: Vec<(Scope, SdkToml)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
    Session,
}

impl ConfigChain {
    pub fn new() -> Self { Self::default() }

    /// Add a layer.  Higher-priority layers should be added last.
    pub fn add(&mut self, scope: Scope, toml: SdkToml) {
        self.layers.push((scope, toml));
    }

    /// Resolve the effective version + scope for a tool.
    /// Session > Project > Global priority.
    pub fn resolve(&self, tool: &str) -> Option<(Scope, &ToolConfig)> {
        // Walk in reverse: last added = highest priority
        for (scope, toml) in self.layers.iter().rev() {
            if let Some(cfg) = toml.get_tool(tool) {
                return Some((*scope, cfg));
            }
        }
        None
    }

    /// All tool names across all layers (deduplicated).
    #[allow(dead_code)]
    pub fn all_tools(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut names = Vec::new();
        for (_, toml) in &self.layers {
            for name in toml.tools.keys() {
                if seen.insert(name.clone()) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    #[allow(dead_code)]
    pub fn get_toml_for_scope(&self, scope: Scope) -> Option<&SdkToml> {
        self.layers.iter().find(|(s, _)| *s == scope).map(|(_, t)| t)
    }

    /// Load from all three scopes (global → project → session).
    pub fn load(paths: &Paths) -> Result<Self> {
        Self::load_from_dir(paths, &paths.working_dir.to_string_lossy())
    }

    /// Load with an explicit CWD for project config discovery.
    pub fn load_from_dir(paths: &Paths, cwd: &str) -> Result<Self> {
        let mut chain = Self::new();
        chain.add(Scope::Global, SdkToml::load(&paths.global_toml)?);

        let cwd_path = std::path::Path::new(cwd);
        if let Some(proj) = crate::paths::find_project_config(cwd_path) {
            chain.add(Scope::Project, SdkToml::load(&proj)?);
        }

        let session_toml = paths.session_toml();
        if session_toml.exists() {
            chain.add(Scope::Session, SdkToml::load(&session_toml)?);
        }
        Ok(chain)
    }

    /// Resolve the effective version string for a tool.
    pub fn resolve_version(&self, tool: &str) -> Option<String> {
        self.resolve(tool).map(|(_, cfg)| cfg.version.clone())
    }

    /// Get the version from a specific scope (not considering higher-priority scopes).
    pub fn get_version_for_scope(&self, tool: &str, scope: Scope) -> Option<String> {
        self.layers
            .iter()
            .find(|(s, _)| *s == scope)
            .and_then(|(_, toml)| toml.get_version(tool))
            .map(|s| s.to_string())
    }

    /// Merge all layers into a single config (later layers win).
    pub fn effective_config(&self) -> SdkToml {
        let mut tools = BTreeMap::new();
        for (_, toml) in &self.layers {
            for (name, cfg) in &toml.tools {
                tools.insert(name.clone(), cfg.clone());
            }
        }
        SdkToml { path: None, tools }
    }
}
