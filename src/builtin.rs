/// Built-in plugin registry.
///
/// Plugin Lua files are embedded in the binary at compile time via `include_str!`.
/// `sdk plugin init` writes them to `~/.sdk/plugin/<name>/` without any network access.
///
/// To update the bundled plugins, copy the new Lua files into `assets/plugins/` and
/// rebuild the SDK binary.

/// A single embedded file: (relative path within the plugin dir, content).
pub type EmbeddedFile = (&'static str, &'static str);

/// All files for one built-in plugin.
pub struct BuiltinPlugin {
    pub name:        &'static str,
    pub description: &'static str,
    pub files:       &'static [EmbeddedFile],
}

// ── Embedded file tables ──────────────────────────────────────────────────────

static JAVA_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/java/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/java/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/java/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/java/hooks/env_keys.lua")),
];

static NODE_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/node/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/node/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/node/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/node/hooks/env_keys.lua")),
];

static PYTHON_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/python/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/python/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/python/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/python/hooks/env_keys.lua")),
];

static GO_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/go/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/go/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/go/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/go/hooks/env_keys.lua")),
];

static GRADLE_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/gradle/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/gradle/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/gradle/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/gradle/hooks/env_keys.lua")),
];

static MAVEN_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",          include_str!("../assets/plugins/maven/metadata.lua")),
    ("hooks/available.lua",   include_str!("../assets/plugins/maven/hooks/available.lua")),
    ("hooks/pre_install.lua", include_str!("../assets/plugins/maven/hooks/pre_install.lua")),
    ("hooks/env_keys.lua",    include_str!("../assets/plugins/maven/hooks/env_keys.lua")),
];

static RUST_FILES: &[EmbeddedFile] = &[
    ("metadata.lua",           include_str!("../assets/plugins/rust/metadata.lua")),
    ("hooks/available.lua",    include_str!("../assets/plugins/rust/hooks/available.lua")),
    ("hooks/pre_install.lua",  include_str!("../assets/plugins/rust/hooks/pre_install.lua")),
    ("hooks/post_install.lua", include_str!("../assets/plugins/rust/hooks/post_install.lua")),
    ("hooks/env_keys.lua",     include_str!("../assets/plugins/rust/hooks/env_keys.lua")),
];

// ── Registry ──────────────────────────────────────────────────────────────────

/// All built-in plugins bundled with this SDK release.
pub static BUILTIN_PLUGINS: &[BuiltinPlugin] = &[
    BuiltinPlugin { name: "java",   description: "Java (Eclipse Temurin / Azul Zulu / Oracle JDK)", files: JAVA_FILES },
    BuiltinPlugin { name: "node",   description: "Node.js (nodejs.org)",                            files: NODE_FILES },
    BuiltinPlugin { name: "python", description: "Python (python-build-standalone)",                files: PYTHON_FILES },
    BuiltinPlugin { name: "go",     description: "Go programming language",                         files: GO_FILES },
    BuiltinPlugin { name: "gradle", description: "Gradle build tool",                               files: GRADLE_FILES },
    BuiltinPlugin { name: "maven",  description: "Apache Maven",                                    files: MAVEN_FILES },
    BuiltinPlugin { name: "rust",   description: "Rust toolchain (via rustup)",                     files: RUST_FILES },
];

/// Look up a built-in plugin by name.
pub fn find(name: &str) -> Option<&'static BuiltinPlugin> {
    BUILTIN_PLUGINS.iter().find(|p| p.name == name)
}

/// Return all built-in plugin names.
pub fn names() -> Vec<&'static str> {
    BUILTIN_PLUGINS.iter().map(|p| p.name).collect()
}

/// Extract embedded plugin files to a destination directory (offline, no network).
pub fn extract_to(plugin: &BuiltinPlugin, dest: &std::path::Path) -> anyhow::Result<()> {
    for (rel_path, content) in plugin.files {
        let target = dest.join(rel_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
    }
    Ok(())
}

