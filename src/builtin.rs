/// Built-in plugin registry.
///
/// Maps plugin name → (git repository URL, subdirectory within that repo).
/// When a user runs `sdk plugin add java` without specifying a source, the SDK
/// clones the repository (shallow), copies the relevant subdirectory into
/// `~/.sdk/plugin/<name>/`, then removes the temporary clone.
pub struct BuiltinEntry {
    /// Git repository URL (supports --depth=1 shallow clone)
    pub repo: &'static str,
    /// Subdirectory within the repo that contains the plugin
    pub subdir: &'static str,
    /// Short description shown in `sdk plugin init --list`
    pub description: &'static str,
}

/// All built-in plugins shipped with this SDK release.
pub const BUILTIN_PLUGINS: &[(&str, BuiltinEntry)] = &[
    ("java", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "java",
        description: "Java (Eclipse Temurin / Azul Zulu / Oracle JDK)",
    }),
    ("node", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "node",
        description: "Node.js (nodejs.org)",
    }),
    ("python", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "python",
        description: "Python (python-build-standalone)",
    }),
    ("go", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "go",
        description: "Go programming language",
    }),
    ("gradle", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "gradle",
        description: "Gradle build tool",
    }),
    ("maven", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "maven",
        description: "Apache Maven",
    }),
    ("rust", BuiltinEntry {
        repo:        "https://github.com/lidongbei/sdk-plugins",
        subdir:      "rust",
        description: "Rust toolchain (via rustup)",
    }),
];

/// Look up a built-in plugin entry by name.
pub fn find(name: &str) -> Option<&'static BuiltinEntry> {
    BUILTIN_PLUGINS.iter().find(|(n, _)| *n == name).map(|(_, e)| e)
}

/// Return all built-in plugin names.
pub fn names() -> Vec<&'static str> {
    BUILTIN_PLUGINS.iter().map(|(n, _)| *n).collect()
}
