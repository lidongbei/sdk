use anyhow::{bail, Result};

use crate::sdk::SdkEnvs;

// ═══════════════════════════════════════════════════════════════════════════════
// Shell env rendering
// ═══════════════════════════════════════════════════════════════════════════════

pub fn render_env(shell: &str, envs: &SdkEnvs) -> Result<String> {
    match shell.to_lowercase().as_str() {
        "bash"  => render_bash(envs),
        "zsh"   => render_zsh(envs),
        "fish"  => render_fish(envs),
        "pwsh" | "powershell" => render_pwsh(envs),
        "nu" | "nushell"      => render_nu(envs),
        other => bail!("Unsupported shell: {}", other),
    }
}

/// Emit the activation script for a shell (eval'd once at shell startup).
pub fn activation_script(shell: &str, binary: &str) -> Result<String> {
    match shell.to_lowercase().as_str() {
        "bash"  => Ok(bash_activation(binary)),
        "zsh"   => Ok(zsh_activation(binary)),
        "fish"  => Ok(fish_activation(binary)),
        "pwsh" | "powershell" => Ok(pwsh_activation(binary)),
        "nu" | "nushell"      => Ok(nu_activation(binary)),
        other => bail!("Unsupported shell: {}", other),
    }
}

// ── Bash ──────────────────────────────────────────────────────────────────────

fn render_bash(envs: &SdkEnvs) -> Result<String> {
    let mut out = String::new();
    for (k, v) in &envs.vars {
        out.push_str(&format!("export {}={}\n", k, shell_quote(v)));
    }
    // Always emit PATH so moving away from a versioned dir resets it cleanly.
    // __SDK_CLEAN_PATH is set by the activation hook; fall back to $PATH when
    // sdk activate is called standalone (outside the hook).
    let extra = envs.paths.join(":");
    if extra.is_empty() {
        out.push_str("export PATH=\"${__SDK_CLEAN_PATH:-$PATH}\"\n");
    } else {
        out.push_str(&format!(
            "export PATH={}:\"${{__SDK_CLEAN_PATH:-$PATH}}\"\n",
            extra
        ));
    }
    Ok(out)
}

fn bash_activation(binary: &str) -> String {
    format!(
        r#"
__sdk_hook() {{
  local new_env
  new_env="$({bin} activate bash "$(pwd)" 2>/dev/null)"
  [ -n "$new_env" ] && eval "$new_env"
}}
if [ -z "$__SDK_INITIALIZED" ]; then
  export __SDK_INITIALIZED=1
  export __SDK_CLEAN_PATH="$PATH"
  export __SDK_CURTMPPATH="${{HOME}}/.sdk/tmp/$$"
  PROMPT_COMMAND="__sdk_hook;${{PROMPT_COMMAND:-:}}"
  __sdk_hook  # activate immediately for the current directory
fi
"#,
        bin = binary
    )
}

// ── Zsh ───────────────────────────────────────────────────────────────────────

fn render_zsh(envs: &SdkEnvs) -> Result<String> {
    render_bash(envs) // same export syntax
}

fn zsh_activation(binary: &str) -> String {
    format!(
        r#"
autoload -Uz add-zsh-hook
__sdk_hook() {{
  local new_env
  new_env="$({bin} activate zsh "$(pwd)" 2>/dev/null)"
  [ -n "$new_env" ] && eval "$new_env"
}}
if [ -z "$__SDK_INITIALIZED" ]; then
  export __SDK_INITIALIZED=1
  export __SDK_CLEAN_PATH="$PATH"
  export __SDK_CURTMPPATH="${{HOME}}/.sdk/tmp/$$"
  add-zsh-hook precmd __sdk_hook
  __sdk_hook  # activate immediately for the current directory
fi
"#,
        bin = binary
    )
}

// ── Fish ──────────────────────────────────────────────────────────────────────

fn render_fish(envs: &SdkEnvs) -> Result<String> {
    let mut out = String::new();
    for (k, v) in &envs.vars {
        out.push_str(&format!("set -gx {} {}\n", k, fish_quote(v)));
    }
    // Rebuild PATH from __SDK_CLEAN_PATH each time to avoid accumulation.
    let clean = "\"$__SDK_CLEAN_PATH\"";
    if envs.paths.is_empty() {
        out.push_str(&format!("set -gx PATH {}\n", clean));
    } else {
        let extra: Vec<String> = envs.paths.iter().map(|p| fish_quote(p)).collect();
        out.push_str(&format!("set -gx PATH {} {}\n", extra.join(" "), clean));
    }
    Ok(out)
}

fn fish_activation(binary: &str) -> String {
    format!(
        r#"
function __sdk_hook --on-variable PWD
    set -l new_env ({bin} activate fish (pwd) 2>/dev/null)
    if test -n "$new_env"
        eval $new_env
    end
end
if not set -q __SDK_INITIALIZED
    set -gx __SDK_INITIALIZED 1
    set -gx __SDK_CLEAN_PATH $PATH
    set -gx __SDK_CURTMPPATH $HOME/.sdk/tmp/(echo %self)
    __sdk_hook  # activate immediately for the current directory
end
"#,
        bin = binary
    )
}

// ── PowerShell ────────────────────────────────────────────────────────────────

fn render_pwsh(envs: &SdkEnvs) -> Result<String> {
    let mut out = String::new();
    for (k, v) in &envs.vars {
        out.push_str(&format!("$env:{} = \"{}\"\n", k, pwsh_escape(v)));
    }
    // Always rebuild PATH from __SDK_CLEAN_PATH to prevent accumulation.
    let clean = "$(if ($env:__SDK_CLEAN_PATH) { $env:__SDK_CLEAN_PATH } else { $env:PATH })";
    if envs.paths.is_empty() {
        out.push_str(&format!("$env:PATH = \"{}\"\n", clean));
    } else {
        let extra = envs.paths.join(";");
        out.push_str(&format!(
            "$env:PATH = \"{};{}\"\n",
            pwsh_escape(&extra),
            clean
        ));
    }
    Ok(out)
}

fn pwsh_activation(binary: &str) -> String {
    format!(
        r#"
function __sdk_hook {{
    $newEnv = & '{bin}' activate pwsh (Get-Location).Path 2>$null
    if ($newEnv) {{
        Invoke-Expression ($newEnv -join "`n")
    }}
}}
if (-not $env:__SDK_INITIALIZED) {{
    $env:__SDK_INITIALIZED = "1"
    $env:__SDK_CLEAN_PATH  = $env:PATH
    $env:__SDK_CURTMPPATH  = "$env:USERPROFILE\.sdk\tmp\$PID"
    $origPrompt = (Get-Item function:prompt).ScriptBlock
    function prompt {{
        __sdk_hook
        & $origPrompt
    }}
    __sdk_hook  # activate immediately for the current directory
}}
"#,
        bin = binary
    )
}

// ── Nushell ───────────────────────────────────────────────────────────────────

fn render_nu(envs: &SdkEnvs) -> Result<String> {
    let mut out = String::new();
    for (k, v) in &envs.vars {
        out.push_str(&format!("$env.{} = \"{}\"\n", k, v.replace('"', "\\\"")));
    }
    if !envs.paths.is_empty() {
        for p in &envs.paths {
            out.push_str(&format!(
                "$env.PATH = ($env.PATH | prepend \"{}\")\n",
                p.replace('"', "\\\"")
            ));
        }
    }
    Ok(out)
}

fn nu_activation(binary: &str) -> String {
    format!(
        r#"
def-env __sdk_hook [] {{
    let new_env = (^'{bin}' activate nu (pwd | str trim))
    if ($new_env | str length) > 0 {{
        nu -c $new_env
    }}
}}
if (env | where name == "__SDK_INITIALIZED" | is-empty) {{
    $env.__SDK_INITIALIZED = "1"
    $env.__SDK_CURTMPPATH = ($env.HOME | path join ".sdk" "tmp" (sys | get host.pid | into string))
    __sdk_hook
}}
"#,
        bin = binary
    )
}

// ═══════════════════════════════════════════════════════════════════════════════
// Quoting helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn shell_quote(s: &str) -> String {
    // Use double-quotes and escape $ ` " \
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{}\"", escaped)
}

fn fish_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

fn pwsh_escape(s: &str) -> String {
    s.replace('"', "`\"")
}
