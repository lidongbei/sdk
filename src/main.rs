use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

mod app;
mod config;
mod paths;
mod plugin;
mod registry;
mod sdk;
mod shell;
mod util;

use app::App;
use config::Scope;

// ═══════════════════════════════════════════════════════════════════════════════
// CLI definition
// ═══════════════════════════════════════════════════════════════════════════════

/// sdk — SDK version manager (no project symlinks)
#[derive(Parser, Debug)]
#[command(name = "sdk", version, author, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Install a SDK version  e.g. `sdk install nodejs@20.0.0`
    #[command(alias = "i")]
    Install {
        /// SDK[@version] to install (defaults to version from .sdk.toml)
        #[arg(value_name = "SDK[@VERSION]")]
        spec: String,
    },

    /// Set the active version for an SDK
    #[command(alias = "u")]
    Use {
        /// SDK[@version] to activate
        #[arg(value_name = "SDK[@VERSION]")]
        spec: String,

        /// Apply change to global config (~/.sdk/.sdk.toml)
        #[arg(long, short = 'g')]
        global: bool,

        /// Apply change to session only (default unless configured otherwise)
        #[arg(long, short = 's')]
        session: bool,

        /// Apply change to the project .sdk.toml in the current directory
        #[arg(long, short = 'p')]
        project: bool,
    },

    /// Uninstall a SDK version  e.g. `sdk uninstall nodejs@20.0.0`
    #[command(aliases = ["rm", "del"])]
    Uninstall {
        #[arg(value_name = "SDK[@VERSION]")]
        spec: String,
    },

    /// Remove an SDK from the active config (does not uninstall)
    Unuse {
        /// SDK name
        sdk: String,

        #[arg(long, short = 'g')]
        global: bool,

        #[arg(long, short = 's')]
        session: bool,
    },

    /// List installed SDK versions
    #[command(alias = "ls")]
    List {
        /// Filter to a specific SDK
        sdk: Option<String>,
    },

    /// Show currently active SDK versions
    #[command(alias = "cur")]
    Current,

    /// List available versions of an SDK
    #[command(alias = "av")]
    Available {
        sdk: String,

        /// Extra arguments forwarded to the plugin
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Add a plugin  e.g. `sdk add nodejs https://github.com/version-fox/vfox-nodejs`
    Add {
        name:   String,
        source: String,
    },

    /// Remove a plugin
    #[command(alias = "plug-rm")]
    Remove { name: String },

    /// Update all installed plugins
    Update,

    /// Show info about an SDK plugin
    Info { sdk: String },

    /// Emit shell activation script  (eval'd by shell RC)
    #[command(hide = true)]
    Activate {
        shell: String,
        /// Current working directory (passed by the shell hook)
        cwd:   Option<String>,
    },

    /// Run a command with a specific SDK version in scope
    Exec {
        sdk:     String,
        version: String,

        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Generate shell completion script  e.g. `sdk completions bash >> ~/.bashrc`
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Diagnose common issues (missing plugins, broken installs, PATH)
    Doctor,

    /// Pin the currently-active SDK version(s) into the project .sdk.toml
    Pin {
        /// Only pin this specific SDK (default: all active)
        sdk: Option<String>,
        /// Explicit version to pin (requires SDK name to be given)
        version: Option<String>,
    },

    /// Remove an SDK entry from the project .sdk.toml
    Unpin {
        /// SDK name to remove from .sdk.toml
        sdk: String,
    },

    /// Show environment variables that will be exported for active SDKs
    Env {
        /// Show only global scope (ignore project .sdk.toml)
        #[arg(long, short = 'g')]
        global: bool,
    },

    /// Link a locally-installed SDK directory into version management
    ///
    /// Registers an existing SDK installation (not downloaded by sdk) as a
    /// named version so it can be activated with `sdk use`.
    ///
    /// Examples:
    ///   sdk link java 21 /usr/lib/jvm/java-21-openjdk
    ///   sdk link nodejs 20 C:\tools\node-20
    #[command(alias = "ln")]
    Link {
        /// SDK name (must have a plugin installed)
        sdk: String,
        /// Version label to assign (e.g. "21" or "21-local")
        version: String,
        /// Path to the existing installation directory
        path: String,
    },

    /// Remove a linked SDK version from version management
    ///
    /// Only works on versions registered with `sdk link`.
    /// Use `sdk uninstall` for versions installed via `sdk install`.
    ///
    /// Example:  sdk unlink java 21
    #[command(alias = "ul")]
    Unlink {
        sdk: String,
        version: String,
    },

    /// Check for newer versions of active SDKs
    Upgrade {
        /// Optional SDK name to limit check
        sdk: Option<String>,

        /// Automatically upgrade to newer versions
        #[arg(long, short = 'y')]
        yes: bool,

        /// Include pre-release versions (alpha, beta, rc, etc.)
        #[arg(long)]
        pre: bool,
    },

    /// List installable versions for an SDK  e.g. `sdk search nodejs`
    ///
    /// The plugin must be installed first (`sdk add <name> <url>`).
    /// An optional filter narrows results by substring match on the version string.
    ///
    /// Examples:
    ///   sdk search nodejs          # all available Node.js versions
    ///   sdk search nodejs 20       # versions containing "20"
    #[command(alias = "s")]
    Search {
        /// SDK name (plugin must be installed)
        sdk: String,

        /// Optional version filter (substring match)
        filter: Option<String>,
    },

    /// Print the shell hook script to enable automatic version switching.
    ///
    /// Add the output to your shell profile:
    ///
    ///   bash:        eval "$(sdk hook bash)"
    ///   zsh:         eval "$(sdk hook zsh)"
    ///   fish:        sdk hook fish | source
    ///   PowerShell:  Invoke-Expression (& sdk hook pwsh | Out-String)
    Hook {
        /// Shell name: bash, zsh, fish, pwsh/powershell, nu/nushell
        shell: String,
    },

    /// View or edit user configuration
    ///
    /// Examples:
    ///   sdk config                        # show all settings
    ///   sdk config get proxy.url          # get one value
    ///   sdk config set proxy.url http://proxy.example.com:8080
    ///   sdk config set proxy.enable true
    Config {
        /// Subcommand: get | set  (omit to show all)
        action: Option<String>,
        /// Config key  (e.g. proxy.url)
        key: Option<String>,
        /// New value  (only for `set`)
        value: Option<String>,
    },

    /// Manage the downloaded archive cache (offline mirror)
    ///
    /// Downloaded SDK archives are kept in `~/.sdk/downloads/` when
    /// `cache.keep_downloads` is `true` (default). These can be reused
    /// for offline installs or shared as a local mirror.
    ///
    /// Examples:
    ///   sdk cache list     # list cached archives with sizes
    ///   sdk cache clean    # remove all cached archives
    Cache {
        /// Subcommand: list | clean
        action: Option<String>,
    },

    /// Manage plugin mirror sources
    ///
    /// Each plugin can define named mirror profiles (e.g. default, china, local).
    /// Use `sdk mirror use <profile>` to switch all plugins to a named profile,
    /// or `sdk mirror set <plugin> <VAR> <url>` to set a custom URL.
    ///
    /// Examples:
    ///   sdk mirror                           # show current mirror settings
    ///   sdk mirror list                      # list all available profiles
    ///   sdk mirror list node                 # list profiles for node plugin
    ///   sdk mirror use china                 # switch all plugins to china profile
    ///   sdk mirror use china node            # switch only node to china profile
    ///   sdk mirror use default               # revert all to official sources
    ///   sdk mirror set node SDK_NODE_MIRROR https://my.mirror/nodejs
    ///   sdk mirror reset                     # remove all mirror overrides
    ///   sdk mirror reset node                # remove node mirror override
    Mirror {
        /// Subcommand: list | use | set | reset  (omit to show current settings)
        action: Option<String>,
        /// Plugin name (for list/use/set/reset targeting one plugin)
        plugin: Option<String>,
        /// Profile name (for `use`) or env var name (for `set`)
        profile_or_var: Option<String>,
        /// URL value (only for `set`)
        url: Option<String>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════════════════

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Suppress ANSI codes if not a TTY or NO_COLOR set
    if std::env::var("NO_COLOR").is_ok() || !is_tty() {
        colored::control::set_override(false);
    }

    let mut app = App::new()?;

    match cli.command {
        Command::Install { spec } => {
            let (sdk_name, version) = parse_spec(&spec)?;
            app.install(&sdk_name, &version)?;
        }

        Command::Use { spec, global, session, project } => {
            let scope = if global {
                Scope::Global
            } else if session {
                Scope::Session
            } else if project {
                Scope::Project
            } else {
                // Use the configured default scope (defaults to "session").
                match app.user_cfg.use_cfg.default_scope.as_str() {
                    "global"  => Scope::Global,
                    "project" => Scope::Project,
                    _         => Scope::Session,
                }
            };
            // If no version was specified (no '@'), show interactive TUI picker.
            if spec.contains('@') {
                let (sdk_name, version) = parse_spec(&spec)?;
                app.use_sdk(&sdk_name, &version, scope)?;
            } else {
                app.use_interactive(&spec, scope)?;
            }
        }

        Command::Uninstall { spec } => {
            let (sdk_name, version) = parse_spec(&spec)?;
            app.uninstall(&sdk_name, &version)?;
        }

        Command::Unuse { sdk, global, session } => {
            let scope = if global  { Scope::Global  }
                        else if session { Scope::Session }
                        else            { Scope::Project };
            app.unuse_sdk(&sdk, scope)?;
        }

        Command::List { sdk } => {
            app.list_installed(sdk.as_deref())?;
        }

        Command::Current => {
            app.current()?;
        }

        Command::Available { sdk, args } => {
            app.available(&sdk, &args)?;
        }

        Command::Add { name, source } => {
            app.add_plugin(&name, &source)?;
        }

        Command::Remove { name } => {
            app.remove_plugin(&name)?;
        }

        Command::Update => {
            app.update_plugins()?;
        }

        Command::Info { sdk } => {
            app.info(&sdk)?;
        }

        Command::Activate { shell, cwd } => {
            // When cwd is absent, emit the static hook setup script
            if let Some(dir) = cwd {
                let env_script = app.activate(&shell, &dir)?;
                print!("{}", env_script);
            } else {
                let binary = std::env::current_exe()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "vfox".to_string());
                let script = shell::activation_script(&shell, &binary)?;
                print!("{}", script);
            }
        }

        Command::Exec { sdk, version, command } => {
            if command.is_empty() {
                eprintln!("Error: no command specified");
                std::process::exit(1);
            }
            let code = app.exec(&sdk, &version, &command)?;
            std::process::exit(code);
        }

        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "sdk",
                &mut std::io::stdout(),
            );
        }

        Command::Doctor => {
            app.doctor()?;
        }

        Command::Pin { sdk, version } => {
            app.pin(sdk.as_deref(), version.as_deref())?;
        }

        Command::Unpin { sdk } => {
            app.unpin(&sdk)?;
        }

        Command::Env { global } => {
            app.env_show(global)?;
        }

        Command::Link { sdk, version, path } => {
            app.link(&sdk, &version, &path)?;
        }

        Command::Unlink { sdk, version } => {
            app.unlink(&sdk, &version)?;
        }

        Command::Upgrade { sdk, yes, pre } => {
            app.upgrade(sdk.as_deref(), yes, pre)?;
        }

        Command::Search { sdk, filter } => {
            app.search(&sdk, filter.as_deref())?;
        }

        Command::Hook { shell } => {
            let binary = std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "sdk".to_string());
            let script = shell::activation_script(&shell, &binary)?;
            print!("{}", script);
        }

        Command::Config { action, key, value } => {
            match action.as_deref() {
                None | Some("show") => app.config_show(),
                Some("get") => {
                    let k = key.as_deref().ok_or_else(|| anyhow::anyhow!("Usage: sdk config get <key>"))?;
                    app.config_get(k)?;
                }
                Some("set") => {
                    let k = key.as_deref().ok_or_else(|| anyhow::anyhow!("Usage: sdk config set <key> <value>"))?;
                    let v = value.as_deref().ok_or_else(|| anyhow::anyhow!("Usage: sdk config set <key> <value>"))?;
                    app.config_set(k, v)?;
                }
                Some(other) => anyhow::bail!("Unknown config action '{}'. Use: get | set", other),
            }
        }

        Command::Cache { action } => {
            match action.as_deref() {
                None | Some("list") => app.cache_list()?,
                Some("clean")       => app.cache_clean()?,
                Some(other) => anyhow::bail!("Unknown cache action '{}'. Use: list | clean", other),
            }
        }

        Command::Mirror { action, plugin, profile_or_var, url } => {
            match action.as_deref() {
                None | Some("show") => app.mirror_show()?,
                Some("list") => app.mirror_list(plugin.as_deref())?,
                Some("use") => {
                    // Positional order: action plugin profile_or_var url
                    // For `sdk mirror use <profile> [plugin-name]`:
                    //   `plugin` receives the profile, `profile_or_var` the optional plugin name
                    let profile = plugin.as_deref()
                        .ok_or_else(|| anyhow::anyhow!("Usage: sdk mirror use <profile> [plugin]"))?;
                    app.mirror_use(profile, profile_or_var.as_deref())?;
                }
                Some("set") => {
                    let plugin_name = plugin.as_deref()
                        .ok_or_else(|| anyhow::anyhow!("Usage: sdk mirror set <plugin> <VAR> <url>"))?;
                    let var = profile_or_var.as_deref()
                        .ok_or_else(|| anyhow::anyhow!("Usage: sdk mirror set <plugin> <VAR> <url>"))?;
                    let url_val = url.as_deref()
                        .ok_or_else(|| anyhow::anyhow!("Usage: sdk mirror set <plugin> <VAR> <url>"))?;
                    app.mirror_set_var(plugin_name, var, url_val)?;
                }
                Some("reset") => app.mirror_reset(plugin.as_deref())?,
                Some(other) => anyhow::bail!("Unknown mirror action '{}'. Use: list | use | set | reset", other),
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Parse "sdk@version" or "sdk" (version defaults to "latest" or current toml).
fn parse_spec(spec: &str) -> Result<(String, String)> {
    if let Some(pos) = spec.find('@') {
        let sdk_name = spec[..pos].to_string();
        let version  = spec[pos + 1..].to_string();
        Ok((sdk_name, version))
    } else {
        // No version specified — try to read from .sdk.toml
        let chain   = config::ConfigChain::load(&paths::Paths::new()?)?;
        let version = chain
            .resolve_version(spec)
            .unwrap_or_else(|| "latest".to_string());
        Ok((spec.to_string(), version))
    }
}

fn is_tty() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe { libc::isatty(std::io::stdout().as_raw_fd()) != 0 }
    }
    #[cfg(not(unix))]
    {
        true
    }
}
