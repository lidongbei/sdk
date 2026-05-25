use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// SDK install directory prefix
pub const VERSION_DIR_PREFIX: &str = "v-";
/// Additional runtime prefix inside a version dir
pub const ADDITION_PREFIX: &str = "add-";
/// Marker file written inside a version dir after successful installation.
/// Absence of this file means the installation was incomplete (interrupted).
pub const INSTALL_COMPLETE_MARKER: &str = ".sdk-complete";
/// Stores the actual (resolved) version when an alias is used as the install identifier.
/// e.g. alias="java8-test" → `.sdk-version` file contains the real version "8.0.492".
pub const VERSION_FILE: &str = ".sdk-version";

/// Session environment variable – inherits session temp path across invocations
pub const ENV_SESSION_DIR: &str = "__SDK_CURTMPPATH";

/// Resolved directory layout for a sdk installation.
///
/// ```text
/// ~/.sdk/                          – sdk user home
///   .sdk.toml                      – global version config  (NO symlinks)
///   config.yaml                    – user settings
///   plugin/
///     <name>/                      – one Lua plugin
///       main.lua  OR
///       metadata.lua + hooks/
///   cache/
///     <sdk>/
///       v-<version>/               – version directory
///         <sdk>-<version>/         – main runtime
///         add-<name>-<ver>/        – additional runtime
///   downloads/
///     <filename>                   – cached downloaded archives (offline mirror)
///   tmp/
///     <pid>/                       – session temp dir
///       .sdk.toml                  – session version config
/// ```
///
/// **Key difference from similar tools:**  
/// Project directories only contain `.sdk.toml`.  There are **no** `.sdk/sdk/`
/// symlink trees in project directories; the `activate` command resolves env
/// variables directly from the cache path.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `~/.sdk`
    pub home: PathBuf,
    /// `~/.sdk/plugin`
    pub plugins: PathBuf,
    /// `~/.sdk/cache`
    pub cache: PathBuf,
    /// `~/.sdk/downloads` – persistent archive cache (offline mirror)
    pub downloads: PathBuf,
    /// `~/.sdk/tmp`
    pub tmp: PathBuf,
    /// `~/.sdk/config.yaml`
    pub user_config: PathBuf,
    /// `~/.sdk/.sdk.toml` – global SDK version file
    pub global_toml: PathBuf,
    /// Current working directory
    pub working_dir: PathBuf,
    /// Session temp directory – either from `__SDK_CURTMPPATH` env or auto-generated
    pub session_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let home_dir =
            dirs::home_dir().context("Cannot determine home directory")?;

        let actual_home = home_dir.join(".sdk");

        let pid = std::process::id();
        let session_dir = if let Ok(path) = std::env::var(ENV_SESSION_DIR) {
            PathBuf::from(path)
        } else {
            actual_home.join("tmp").join(pid.to_string())
        };

        let working_dir =
            std::env::current_dir().context("Cannot determine current directory")?;

        let paths = Self {
            plugins:     actual_home.join("plugin"),
            cache:       actual_home.join("cache"),
            downloads:   actual_home.join("downloads"),
            tmp:         actual_home.join("tmp"),
            user_config: actual_home.join("config.yaml"),
            global_toml: actual_home.join(".sdk.toml"),
            session_dir: session_dir.clone(),
            working_dir,
            home:        actual_home,
        };

        // Ensure essential directories exist
        std::fs::create_dir_all(&paths.home)?;
        std::fs::create_dir_all(&paths.plugins)?;
        std::fs::create_dir_all(&paths.cache)?;
        std::fs::create_dir_all(&paths.downloads)?;
        std::fs::create_dir_all(&paths.tmp)?;
        // Session dir is created on demand
        std::fs::create_dir_all(&session_dir)?;

        Ok(paths)
    }

    // ── SDK paths ─────────────────────────────────────────────────────────────

    /// `~/.sdk/cache/<sdk>`
    pub fn sdk_cache_dir(&self, sdk: &str) -> PathBuf {
        self.cache.join(sdk)
    }

    /// `~/.sdk/cache/<sdk>/v-<version>` – version directory
    pub fn version_dir(&self, sdk: &str, version: &str) -> PathBuf {
        self.sdk_cache_dir(sdk)
            .join(format!("{}{}", VERSION_DIR_PREFIX, version))
    }

    /// `~/.sdk/cache/<sdk>/v-<version>/.sdk-complete` – installation complete marker
    pub fn install_complete_marker(&self, sdk: &str, version: &str) -> PathBuf {
        self.version_dir(sdk, version).join(INSTALL_COMPLETE_MARKER)
    }

    /// `~/.sdk/cache/<sdk>/v-<version>/.sdk-version` – actual version when alias is used
    pub fn version_file(&self, sdk: &str, version: &str) -> PathBuf {
        self.version_dir(sdk, version).join(VERSION_FILE)
    }

    /// `~/.sdk/cache/<sdk>/v-<version>/<sdk>-<version>` – main runtime path
    #[allow(dead_code)]
    pub fn runtime_path(&self, sdk: &str, version: &str) -> PathBuf {
        self.version_dir(sdk, version)
            .join(format!("{}-{}", sdk, version))
    }

    /// `~/.sdk/cache/<sdk>/v-<version>/add-<name>-<ver>` – additional runtime
    #[allow(dead_code)]
    pub fn addition_path(&self, sdk: &str, version: &str, name: &str, add_ver: &str) -> PathBuf {
        self.version_dir(sdk, version)
            .join(format!("{}{}-{}", ADDITION_PREFIX, name, add_ver))
    }

    /// `~/.sdk/cache/<sdk>/v-<version>/.link` – external path marker for linked installs
    pub fn link_file(&self, sdk: &str, version: &str) -> PathBuf {
        self.version_dir(sdk, version).join(".link")
    }

    // ── Plugin paths ──────────────────────────────────────────────────────────

    /// `~/.sdk/plugin/<name>`
    pub fn plugin_dir(&self, name: &str) -> PathBuf {
        self.plugins.join(name)
    }

    // ── Config paths ──────────────────────────────────────────────────────────

    /// `.sdk.toml` in the project root (current working directory).
    /// This is the **only** sdk artifact placed in a project directory.
    #[allow(dead_code)]
    pub fn project_toml(&self) -> PathBuf {
        find_config_path(&self.working_dir)
            .unwrap_or_else(|| self.working_dir.join(".sdk.toml"))
    }

    /// `.sdk.toml` inside the session temp directory
    pub fn session_toml(&self) -> PathBuf {
        self.session_dir.join(".sdk.toml")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Returns `true` if `path` is inside an sdk-managed directory.
    #[allow(dead_code)]
    pub fn is_sdk_path(path: &Path) -> bool {
        path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == ".sdk"
        })
    }

    /// Enumerate all installed versions for an SDK.
    pub fn installed_versions(&self, sdk: &str) -> Vec<String> {
        let sdk_dir = self.sdk_cache_dir(sdk);
        if !sdk_dir.exists() {
            return Vec::new();
        }
        let mut versions: Vec<String> = std::fs::read_dir(&sdk_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && name.starts_with(VERSION_DIR_PREFIX)
                {
                    let ver = name[VERSION_DIR_PREFIX.len()..].to_string();
                    let path = e.path();
                    // Complete if: marker exists (new) OR directory is non-empty (legacy)
                    let marker = path.join(INSTALL_COMPLETE_MARKER);
                    let complete = marker.exists()
                        || std::fs::read_dir(&path).map(|mut d| d.next().is_some()).unwrap_or(false);
                    if complete { Some(ver) } else { None }
                } else {
                    None
                }
            })
            .collect();
        versions.sort_by(|a, b| b.cmp(a));
        versions
    }

    /// Returns names of all installed plugins.
    pub fn installed_plugins(&self) -> Vec<String> {
        std::fs::read_dir(&self.plugins)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    Some(e.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Look for an existing config file in `dir`.  Prefers `.sdk.toml`,
/// falls back to `sdk.toml`.
pub fn find_config_path(dir: &Path) -> Option<PathBuf> {
    let preferred = dir.join(".sdk.toml");
    if preferred.exists() {
        return Some(preferred);
    }
    let alt = dir.join("sdk.toml");
    if alt.exists() {
        return Some(alt);
    }
    None
}

/// Walk up from `start` looking for a `.sdk.toml` / `sdk.toml` file.
/// Returns the first found (project-level config).
pub fn find_project_config(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if let Some(p) = find_config_path(dir) {
            return Some(p);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}
