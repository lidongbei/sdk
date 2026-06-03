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

    // Incremental PATH management:
    //   1. Remove previous SDK entries from PATH
    //   2. Split remaining PATH into "external" (not in __SDK_CLEAN_PATH) and "clean" parts
    //   3. Rebuild as: external + sdk + clean
    // This preserves foreign PATH modifications (e.g. Python venv) and gives them
    // higher priority than SDK-managed paths.

    out.push_str(
        "# Remove previous SDK entries from PATH\n\
         if [ -n \"${__SDK_PREV_PATHS:-}\" ]; then\n\
           _sdk_old_ifs=\"$IFS\"\n\
           IFS=':'\n\
           set -- $__SDK_PREV_PATHS\n\
           for _sdk_r; do\n\
             PATH=\":$PATH:\"\n\
             PATH=\"${PATH//:$_sdk_r:/:}\"\n\
             PATH=\"${PATH#:}\"; PATH=\"${PATH%:}\"\n\
           done\n\
           IFS=\"$_sdk_old_ifs\"\n\
         fi\n\
         # Separate external entries (not in __SDK_CLEAN_PATH) from clean entries\n\
         _sdk_external=\"\"\n\
         _sdk_clean_part=\"\"\n\
         if [ -n \"${__SDK_CLEAN_PATH:-}\" ]; then\n\
           _sdk_old_ifs=\"$IFS\"\n\
           IFS=':'\n\
           for _sdk_p in $PATH; do\n\
             [ -z \"$_sdk_p\" ] && continue\n\
             _sdk_is_clean=0\n\
             IFS=':'\n\
             for _sdk_c in $__SDK_CLEAN_PATH; do\n\
               [ \"$_sdk_p\" = \"$_sdk_c\" ] && { _sdk_is_clean=1; break; }\n\
             done\n\
             IFS=':'\n\
             if [ $_sdk_is_clean -eq 1 ]; then\n\
               _sdk_clean_part=\"${_sdk_clean_part:+$_sdk_clean_part:}$_sdk_p\"\n\
             else\n\
               _sdk_external=\"${_sdk_external:+$_sdk_external:}$_sdk_p\"\n\
             fi\n\
           done\n\
           IFS=\"$_sdk_old_ifs\"\n\
         else\n\
           _sdk_clean_part=\"$PATH\"\n\
         fi\n"
    );

    if !envs.paths.is_empty() {
        let extra = envs.paths.join(":");
        let colon_extra = format!(":{}", extra);
        out.push_str(&format!(
            "export PATH=\"$_sdk_external{}:$_sdk_clean_part\"\n",
            colon_extra
        ));
        // Strip leading/trailing colons that may result from empty external part
        out.push_str("export PATH=\"${PATH#:}\"; export PATH=\"${PATH%:}\"\n");
        out.push_str(&format!("export __SDK_PREV_PATHS=\"{}\"\n", extra));
    } else {
        out.push_str(
            "export PATH=\"$_sdk_external:$_sdk_clean_part\"\n\
             export PATH=\"${PATH#:}\"; export PATH=\"${PATH%:}\"\n\
             export __SDK_PREV_PATHS=\"\"\n"
        );
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
# Register the hook in every new shell (PROMPT_COMMAND is not inherited)
case "${{PROMPT_COMMAND:-}}" in
  *__sdk_hook*) ;;
  *) PROMPT_COMMAND="__sdk_hook;${{PROMPT_COMMAND:-:}}" ;;
esac
if [ -z "${{__SDK_INITIALIZED:-}}" ]; then
  export __SDK_INITIALIZED=1
  export __SDK_CLEAN_PATH="$PATH"
  export __SDK_CURTMPPATH="${{HOME}}/.sdk/tmp/$$"
fi
__sdk_hook  # activate immediately for the current directory
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
# Register the hook in every new shell (add-zsh-hook is idempotent)
add-zsh-hook precmd __sdk_hook
if [ -z "${{__SDK_INITIALIZED:-}}" ]; then
  export __SDK_INITIALIZED=1
  export __SDK_CLEAN_PATH="$PATH"
  export __SDK_CURTMPPATH="${{HOME}}/.sdk/tmp/$$"
fi
__sdk_hook  # activate immediately for the current directory
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

    // Incremental PATH management:
    //   1. Remove previous SDK entries from PATH
    //   2. Split remaining PATH into "external" and "clean" parts
    //   3. Rebuild as: external + sdk + clean
    // Preserves foreign PATH modifications (e.g. Python venv) above SDK paths.

    out.push_str(
        "# Remove previous SDK entries from PATH\n\
         if set -q __SDK_PREV_PATHS; and test -n \"$__SDK_PREV_PATHS\"\n\
             for _sdk_r in (string split : $__SDK_PREV_PATHS)\n\
                 if test -n \"$_sdk_r\"; and contains -- \"$_sdk_r\" $PATH\n\
                     set -l _sdk_idx (contains -i -- \"$_sdk_r\" $PATH)\n\
                     set -e PATH[$_sdk_idx]\n\
                 end\n\
             end\n\
         end\n\
         # Separate external entries from clean entries\n\
         set -l _sdk_external\n\
         set -l _sdk_clean_part\n\
         if set -q __SDK_CLEAN_PATH; and test -n \"$__SDK_CLEAN_PATH\"\n\
             for _sdk_p in $PATH\n\
                 if test -n \"$_sdk_p\"; and contains -- \"$_sdk_p\" (string split : $__SDK_CLEAN_PATH)\n\
                     set -a _sdk_clean_part \"$_sdk_p\"\n\
                 else\n\
                     set -a _sdk_external \"$_sdk_p\"\n\
                 end\n\
             end\n\
         else\n\
             set _sdk_clean_part $PATH\n\
         end\n"
    );

    if !envs.paths.is_empty() {
        let extra: Vec<String> = envs.paths.iter().map(|p| fish_quote(p)).collect();
        out.push_str(&format!(
            "set -gx PATH $_sdk_external {} $_sdk_clean_part\n",
            extra.join(" ")
        ));
        out.push_str(&format!(
            "set -gx __SDK_PREV_PATHS \"{}\"\n",
            envs.paths.join(":")
        ));
    } else {
        out.push_str(
            "set -gx PATH $_sdk_external $_sdk_clean_part\n\
             set -gx __SDK_PREV_PATHS \"\"\n"
        );
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

    // Incremental PATH management:
    //   1. Remove previous SDK entries from PATH
    //   2. Split remaining PATH into "external" (not in __SDK_CLEAN_PATH) and "clean" parts
    //   3. Rebuild as: external + sdk + clean
    // This preserves foreign PATH modifications (e.g. Python venv added by Activate.ps1)
    // and gives them higher priority than SDK-managed paths.

    out.push_str(
        "# Remove previous SDK entries from PATH\n\
         if ($env:__SDK_PREV_PATHS) {\n\
             $_sdk_prev = ($env:__SDK_PREV_PATHS -split ';') | Where-Object { $_ }\n\
             $_sdk_cur = $env:PATH -split ';'\n\
             $env:PATH = ($_sdk_cur | Where-Object { $_ -notin $_sdk_prev }) -join ';'\n\
         }\n\
         # Separate external entries (not in clean path) from clean entries\n\
         if ($env:__SDK_CLEAN_PATH) {\n\
             $_sdk_cleanSet = ($env:__SDK_CLEAN_PATH -split ';') | Where-Object { $_ }\n\
             $_sdk_all = $env:PATH -split ';' | Where-Object { $_ }\n\
             $_sdk_external = @($_sdk_all | Where-Object { $_ -notin $_sdk_cleanSet })\n\
             $_sdk_cleanPart = @($_sdk_all | Where-Object { $_ -in $_sdk_cleanSet })\n\
         } else {\n\
             $_sdk_external = @()\n\
             $_sdk_cleanPart = @($env:PATH -split ';' | Where-Object { $_ })\n\
         }\n"
    );

    if !envs.paths.is_empty() {
        let extra: Vec<String> = envs.paths.iter()
            .map(|p| format!("\"{}\"", pwsh_escape(p)))
            .collect();
        let extra_str = extra.join(", ");
        out.push_str(&format!(
            "$_sdk_new = @({})\n", extra_str
        ));
        out.push_str(
            "$env:PATH = (@($_sdk_external) + $_sdk_new + @($_sdk_cleanPart) | Where-Object { $_ }) -join ';'\n\
             $env:__SDK_PREV_PATHS = ($_sdk_new -join ';')\n"
        );
    } else {
        out.push_str(
            "$env:PATH = (@($_sdk_external) + @($_sdk_cleanPart) | Where-Object { $_ }) -join ';'\n\
             $env:__SDK_PREV_PATHS = \"\"\n"
        );
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

    // Incremental PATH management:
    //   1. Remove previous SDK entries from PATH
    //   2. Split remaining PATH into "external" and "clean" parts
    //   3. Rebuild as: external + sdk + clean
    // Preserves foreign PATH modifications above SDK paths.

    out.push_str(
        "# Remove previous SDK entries from PATH\n\
         if (\'__SDK_PREV_PATHS\' in $env) and ($env.__SDK_PREV_PATHS | str length) > 0 {{\n\
             let prev = $env.__SDK_PREV_PATHS | split row (char esep)\n\
             $env.PATH = ($env.PATH | split row (char esep) | where {{ |p| $p not-in $prev }}) | str join (char esep)\n\
         }}\n\
         # Separate external entries from clean entries\n\
         let external = if (\'__SDK_CLEAN_PATH\' in $env) and ($env.__SDK_CLEAN_PATH | str length) > 0 {{\n\
             let clean_set = $env.__SDK_CLEAN_PATH | split row (char esep)\n\
             let all = $env.PATH | split row (char esep)\n\
             let ext = $all | where {{ |p| $p not-in $clean_set }}\n\
             let cln = $all | where {{ |p| $p in $clean_set }}\n\
             {{ external: $ext, clean: $cln }}\n\
         }} else {{\n\
             {{ external: [], clean: ($env.PATH | split row (char esep)) }}\n\
         }}\n"
    );

    if !envs.paths.is_empty() {
        let paths_nu: Vec<String> = envs.paths.iter()
            .map(|p| format!("\"{}\"", p.replace('"', "\\\"")))
            .collect();
        let paths_str = paths_nu.join(", ");
        out.push_str(&format!(
            "let sdk_new = [{}]\n\
             $env.PATH = ($external.external | append $sdk_new | append $external.clean | str join (char esep))\n\
             $env.__SDK_PREV_PATHS = ($sdk_new | str join (char esep))\n",
            paths_str
        ));
    } else {
        out.push_str(
            "$env.PATH = ($external.external | append $external.clean | str join (char esep))\n\
             $env.__SDK_PREV_PATHS = \"\"\n"
        );
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

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_pwsh_incremental_path() {
        let mut envs = SdkEnvs::default();
        envs.paths = vec![
            r"C:\Users\user\.sdk\cache\python\v-3.12.0\python-3.12.0".to_string(),
            r"C:\Users\user\.sdk\cache\python\v-3.12.0\python-3.12.0\Scripts".to_string(),
        ];
        envs.vars.insert("SDK_PYTHON_HOME".to_string(), r"C:\Users\user\.sdk\cache\python\v-3.12.0".to_string());

        let out = render_pwsh(&envs).unwrap();

        // Should contain non-PATH env var
        assert!(out.contains("$env:SDK_PYTHON_HOME"), "should set SDK_PYTHON_HOME");

        // Should NOT rebuild PATH from __SDK_CLEAN_PATH directly (the old behavior)
        assert!(!out.contains("$env:PATH = \"$env:__SDK_CLEAN_PATH\""), "should NOT rebuild from clean path directly");

        // Should use __SDK_PREV_PATHS to track previous entries
        assert!(out.contains("__SDK_PREV_PATHS"), "should track previous SDK paths");

        // Should separate external from clean entries
        assert!(out.contains("_sdk_cleanSet"), "should reference clean path set");
        assert!(out.contains("_sdk_external"), "should separate external entries");
        assert!(out.contains("_sdk_cleanPart"), "should separate clean entries");

        // Should insert SDK paths between external and clean parts
        assert!(out.contains("$_sdk_external"), "should put external entries first");
        assert!(out.contains("$_sdk_new"), "should put SDK paths in the middle");
        assert!(out.contains("$_sdk_cleanPart"), "should put clean entries last");

        // Verify the rebuild order: external + sdk + clean
        let rebuild_line = out.lines()
            .find(|l| l.contains("$env:PATH =") && l.contains("_sdk_external"))
            .unwrap();
        let ext_pos = rebuild_line.find("_sdk_external").unwrap();
        let new_pos = rebuild_line.find("_sdk_new").unwrap();
        let clean_pos = rebuild_line.find("_sdk_cleanPart").unwrap();
        assert!(ext_pos < new_pos, "external should come before sdk (venv > sdk)");
        assert!(new_pos < clean_pos, "sdk should come before clean (sdk > system)");
    }

    #[test]
    fn test_render_pwsh_no_sdk_paths_cleans_up() {
        let mut envs = SdkEnvs::default();
        envs.vars.insert("SDK_PYTHON_HOME".to_string(), "".to_string());

        let out = render_pwsh(&envs).unwrap();

        // When no SDK paths are active, __SDK_PREV_PATHS should be cleared
        assert!(out.contains("$env:__SDK_PREV_PATHS = \"\""), "should clear previous paths when no SDKs active");

        // Should still have the external+clean rebuild (without sdk_new)
        assert!(out.contains("_sdk_external"), "should preserve external entries");
        assert!(out.contains("_sdk_cleanPart"), "should preserve clean entries");
    }

    #[test]
    fn test_render_bash_incremental_path() {
        let mut envs = SdkEnvs::default();
        envs.paths = vec![
            "/home/user/.sdk/cache/node/v-20.0.0/node-20.0.0/bin".to_string(),
        ];

        let out = render_bash(&envs).unwrap();

        // Should use incremental approach, not rebuild from __SDK_CLEAN_PATH
        assert!(out.contains("__SDK_PREV_PATHS"), "bash should track previous paths");
        assert!(out.contains("_sdk_external"), "bash should separate external entries");
        assert!(out.contains("_sdk_clean_part"), "bash should separate clean entries");

        // Should strip leading/trailing colons
        assert!(out.contains("${PATH#:}"), "should strip leading colon");
        assert!(out.contains("${PATH%:}"), "should strip trailing colon");
    }

    #[test]
    fn test_render_fish_incremental_path() {
        let mut envs = SdkEnvs::default();
        envs.paths = vec![
            "/home/user/.sdk/cache/go/v-1.22.0/go-1.22.0/bin".to_string(),
        ];

        let out = render_fish(&envs).unwrap();

        // Should use incremental approach
        assert!(out.contains("__SDK_PREV_PATHS"), "fish should track previous paths");
        assert!(out.contains("_sdk_external"), "fish should separate external entries");
        assert!(out.contains("_sdk_clean_part"), "fish should separate clean entries");
    }

    #[test]
    fn test_render_nu_incremental_path() {
        let mut envs = SdkEnvs::default();
        envs.paths = vec![
            "/home/user/.sdk/cache/rust/v-1.80.0/rust-1.80.0/bin".to_string(),
        ];

        let out = render_nu(&envs).unwrap();

        // Should use incremental approach
        assert!(out.contains("__SDK_PREV_PATHS"), "nu should track previous paths");
        assert!(out.contains("external"), "nu should separate external entries");
    }
}
