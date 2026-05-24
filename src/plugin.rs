use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use mlua::{
    Error as LuaError,
    Function as LuaFunction,
    Lua,
    LuaSerdeExt,
    Result as LuaResult,
    Table as LuaTable,
    Value as LuaValue,
};
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::UserConfig;

/// Deserialize a field that the Lua plugin may return as either a string or a number.
fn de_string_or_num<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    use serde::de::Unexpected;
    struct Vis;
    impl<'de> serde::de::Visitor<'de> for Vis {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("string or number")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<String, E> { Ok(v.to_string()) }
        fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<String, E> { Ok(v) }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<String, E> { Ok(v.to_string()) }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<String, E> { Ok(v.to_string()) }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<String, E> { Ok(v.to_string()) }
        fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<String, E> { Ok(v.to_string()) }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<String, E> { Ok(String::new()) }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<String, E> { Ok(String::new()) }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> std::result::Result<String, D2::Error> {
            d.deserialize_any(Vis)
        }
        fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> std::result::Result<String, E> {
            String::from_utf8(v.to_vec()).map_err(|_| E::invalid_value(Unexpected::Bytes(v), &self))
        }
    }
    d.deserialize_any(Vis)
}

fn de_string_or_num_default<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    de_string_or_num(d)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Hook data models
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
pub struct AvailableCtx {
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AvailableItem {
    pub version:  String,
    #[serde(default)]
    pub note:     String,
    #[allow(dead_code)]
    #[serde(default)]
    pub addition: Vec<AdditionItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdditionItem {
    pub name:    String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub note:    String,
}

/// A named mirror profile defined by a plugin in `PLUGIN.mirrors`.
#[derive(Debug, Clone)]
pub struct MirrorProfile {
    pub name:        String,
    pub description: String,
    /// Environment variables this profile sets (e.g. `SDK_NODE_MIRROR`).
    pub vars:        std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct PreInstallCtx {
    pub version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PreInstallResult {
    pub name:     Option<String>,
    #[serde(deserialize_with = "de_string_or_num")]
    pub version:  String,
    /// Download URL or local file path.  Empty means no download needed.
    #[serde(rename = "url", default)]
    pub url:      String,
    /// Optional fallback URL to try if `url` download fails or returns an invalid archive.
    /// Useful for mirror sources: set `url` to the mirror, `fallback_url` to the official CDN.
    #[serde(rename = "fallback_url", default)]
    pub fallback_url: String,
    #[serde(default)]
    pub headers:  HashMap<String, String>,
    #[serde(default, deserialize_with = "de_string_or_num_default")]
    pub note:     String,
    #[serde(default)]
    pub sha256:   String,
    #[serde(default)]
    pub sha512:   String,
    #[serde(default)]
    pub sha1:     String,
    #[serde(default)]
    pub md5:      String,
    #[serde(default)]
    pub addition: Vec<PreInstallResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name:    String,
    pub version: String,
    /// Absolute path to the runtime directory
    pub path:    String,
    #[serde(default)]
    pub note:    String,
}

#[derive(Debug, Serialize)]
pub struct PostInstallCtx {
    #[serde(rename = "rootPath")]
    pub root_path: String,
    #[serde(rename = "sdkInfo")]
    pub sdk_info:  HashMap<String, InstalledPackage>,
}

#[derive(Debug, Serialize)]
pub struct EnvKeysCtx {
    pub main:     InstalledPackage,
    /// Legacy field kept for backward compatibility
    pub path:     String,
    #[serde(rename = "sdkInfo")]
    pub sdk_info: HashMap<String, InstalledPackage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnvKeyItem {
    pub key:   String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct PreUseCtx {
    pub cwd:              String,
    pub scope:            String,
    pub version:          String,
    #[serde(rename = "previousVersion")]
    pub previous_version: String,
    #[serde(rename = "installedSdks")]
    pub installed_sdks:   HashMap<String, InstalledPackage>,
}

#[derive(Debug, Deserialize)]
pub struct PreUseResult {
    pub version: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ParseLegacyFileCtx {
    pub filepath: String,
    pub filename: String,
    #[serde(default)]
    pub strategy: String,
}

#[derive(Debug, Deserialize)]
pub struct ParseLegacyFileResult {
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct PreUninstallCtx {
    pub main:     InstalledPackage,
    #[serde(rename = "sdkInfo")]
    pub sdk_info: HashMap<String, InstalledPackage>,
}

/// Metadata extracted from the `PLUGIN` global table in Lua.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginMetadata {
    pub name:                String,
    #[serde(default)]
    pub version:             String,
    #[serde(default)]
    pub description:         String,
    #[serde(rename = "updateUrl", default)]
    pub update_url:          String,
    #[serde(rename = "homepage", default)]
    pub homepage:            String,
    #[serde(rename = "minRuntimeVersion", default)]
    pub min_runtime_version: String,
    #[serde(rename = "legacyFilenames", default)]
    pub legacy_filenames:    Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lua plugin
// ═══════════════════════════════════════════════════════════════════════════════

const PLUGIN_KEY: &str    = "PLUGIN";
const OS_TYPE_KEY: &str   = "OS_TYPE";
const ARCH_TYPE_KEY: &str = "ARCH_TYPE";
const RUNTIME_KEY: &str   = "RUNTIME";
const NAVIGATOR_KEY: &str = "SDK_NAVIGATOR";

pub struct LuaPlugin {
    lua:          Lua,
    pub metadata: PluginMetadata,
    #[allow(dead_code)]
    pub dir:      PathBuf,
}

impl LuaPlugin {
    /// Load a plugin from `plugin_dir`.
    pub fn load(plugin_dir: &Path, user_cfg: &UserConfig) -> Result<Self> {
        let lua = Lua::new();

        // Register Lua modules and globals
        setup_globals(&lua, plugin_dir, user_cfg)?;

        let main_lua = plugin_dir.join("main.lua");
        if main_lua.exists() {
            // Legacy single-file plugin
            limit_package_path(&lua, &[plugin_dir.join("?.lua")])?;
            lua.load(
                std::fs::read_to_string(&main_lua)
                    .with_context(|| format!("reading {}", main_lua.display()))?,
            )
            .set_name("main.lua")
            .exec()
            .with_context(|| format!("executing {}", main_lua.display()))?;
        } else {
            // Multi-file plugin: metadata.lua + hooks/
            let hooks_dir = plugin_dir.join("hooks");
            let lib_dir   = plugin_dir.join("lib");
            limit_package_path(&lua, &[hooks_dir.join("?.lua"), lib_dir.join("?.lua")])?;

            let metadata_lua = plugin_dir.join("metadata.lua");
            if !metadata_lua.exists() {
                bail!("Plugin invalid: missing metadata.lua in {}", plugin_dir.display());
            }
            lua.load(std::fs::read_to_string(&metadata_lua)?)
                .set_name("metadata.lua")
                .exec()
                .context("executing metadata.lua")?;

            // Load hook files
            for (func_name, filename, required) in HOOK_FILES {
                let path = hooks_dir.join(format!("{}.lua", filename));
                if !required && !path.exists() {
                    continue;
                }
                lua.load(
                    std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?,
                )
                .set_name(*filename)
                .exec()
                .with_context(|| format!("executing hook {}", func_name))?;
            }
        }

        // Set OS/arch globals AFTER loading scripts (so plugins can't override them)
        let os_type   = get_os_type();
        let arch_type = get_arch_type();
        lua.globals().set(OS_TYPE_KEY, os_type.as_str())?;
        lua.globals().set(ARCH_TYPE_KEY, arch_type.as_str())?;

        let runtime = lua.create_table()?;
        runtime.set("osType",        os_type.as_str())?;
        runtime.set("archType",      arch_type.as_str())?;
        runtime.set("version",       env!("CARGO_PKG_VERSION"))?;
        runtime.set("pluginDirPath", plugin_dir.to_string_lossy().as_ref())?;
        lua.globals().set(RUNTIME_KEY, runtime)?;

        // Extract metadata from PLUGIN global
        let plugin_obj: LuaTable = lua
            .globals()
            .get(PLUGIN_KEY)
            .context("PLUGIN global not found – is this a valid sdk plugin?")?;

        // Extract metadata from PLUGIN global — read fields individually to
        // avoid serde failing on function values attached to the PLUGIN table.
        let metadata = {
            let name: String = plugin_obj.get("name").unwrap_or_default();
            let version: String = plugin_obj.get("version").unwrap_or_default();
            let description: String = plugin_obj.get("description").unwrap_or_default();
            let update_url: String = plugin_obj.get("updateUrl").unwrap_or_default();
            let homepage: String = plugin_obj.get("homepage").unwrap_or_default();
            let min_runtime_version: String = plugin_obj.get("minRuntimeVersion").unwrap_or_default();
            let legacy_filenames: Vec<String> = plugin_obj
                .get::<LuaValue>("legacyFilenames")
                .ok()
                .and_then(|v| if let LuaValue::Table(t) = v { Some(t) } else { None })
                .map(|t| {
                    t.sequence_values::<String>()
                        .flatten()
                        .collect()
                })
                .unwrap_or_default();
            PluginMetadata { name, version, description, update_url, homepage, min_runtime_version, legacy_filenames }
        };

        // Apply plugin-specific env defaults (e.g. auto UV build for Python on Windows)
        inject_plugin_env_defaults(&lua, &metadata.name)?;

        Ok(Self { lua, metadata, dir: plugin_dir.to_owned() })
    }

    /// Whether the plugin exports a given hook function.
    pub fn has_hook(&self, name: &str) -> bool {
        let Ok(plugin): Result<LuaTable, _> = self.lua.globals().get(PLUGIN_KEY) else {
            return false;
        };
        !matches!(plugin.get::<LuaValue>(name), Ok(LuaValue::Nil) | Err(_))
    }

    // ── Hook calls ────────────────────────────────────────────────────────────

    pub fn call_available(&self, args: &[String]) -> Result<Vec<AvailableItem>> {
        let ctx = AvailableCtx { args: args.to_vec() };
        self.call_hook::<_, Vec<AvailableItem>>("Available", &ctx)
    }

    pub fn call_pre_install(&self, version: &str) -> Result<PreInstallResult> {
        let ctx = PreInstallCtx { version: version.to_string() };
        self.call_hook::<_, PreInstallResult>("PreInstall", &ctx)
    }

    pub fn call_post_install(
        &self,
        root_path: &str,
        sdk_info: HashMap<String, InstalledPackage>,
    ) -> Result<()> {
        if !self.has_hook("PostInstall") {
            return Ok(());
        }
        let ctx = PostInstallCtx { root_path: root_path.to_string(), sdk_info };
        self.call_hook_void("PostInstall", &ctx)
    }

    pub fn call_env_keys(
        &self,
        main: InstalledPackage,
        sdk_info: HashMap<String, InstalledPackage>,
    ) -> Result<Vec<EnvKeyItem>> {
        let path = main.path.clone();
        let ctx  = EnvKeysCtx { main, path, sdk_info };
        self.call_hook::<_, Vec<EnvKeyItem>>("EnvKeys", &ctx)
    }

    pub fn call_pre_use(
        &self,
        version: &str,
        previous: &str,
        scope: &str,
        cwd: &str,
        installed: HashMap<String, InstalledPackage>,
    ) -> Result<Option<PreUseResult>> {
        if !self.has_hook("PreUse") {
            return Ok(None);
        }
        let ctx = PreUseCtx {
            version:          version.to_string(),
            previous_version: previous.to_string(),
            scope:            scope.to_string(),
            cwd:              cwd.to_string(),
            installed_sdks:   installed,
        };
        match self.call_hook::<_, PreUseResult>("PreUse", &ctx) {
            Ok(r) => Ok(Some(r)),
            Err(e) if is_no_result(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn call_parse_legacy_file(
        &self,
        filepath: &str,
        filename: &str,
        installed_versions: &[String],
    ) -> Result<Option<ParseLegacyFileResult>> {
        if !self.has_hook("ParseLegacyFile") {
            return Ok(None);
        }

        // Build ctx table manually to inject getInstalledVersions() Lua function
        let ctx = self.lua.create_table()?;
        ctx.set("filepath", filepath)?;
        ctx.set("filename", filename)?;
        ctx.set("strategy", "latest_installed")?;

        let versions_copy: Vec<String> = installed_versions.to_vec();
        ctx.set(
            "getInstalledVersions",
            self.lua.create_function(move |lua, _: ()| {
                let tbl = lua.create_table()?;
                for (i, v) in versions_copy.iter().enumerate() {
                    tbl.set(i + 1, v.as_str())?;
                }
                Ok(tbl)
            })?,
        )?;

        let plugin: LuaTable = self.lua.globals().get(PLUGIN_KEY)?;
        let method: LuaFunction = plugin
            .get("ParseLegacyFile")
            .with_context(|| "hook 'ParseLegacyFile' not found in plugin")?;
        let result: LuaValue = method
            .call((plugin.clone(), ctx))
            .context("calling plugin hook 'ParseLegacyFile'")?;

        if matches!(result, LuaValue::Nil) {
            return Ok(None);
        }
        match self.lua.from_value::<ParseLegacyFileResult>(result) {
            Ok(r) if r.version.is_empty() => Ok(None),
            Ok(r) => Ok(Some(r)),
            Err(e) if is_no_result(&anyhow::anyhow!("{}", e)) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("{}", e)),
        }
    }

    pub fn call_pre_uninstall(
        &self,
        main: InstalledPackage,
        sdk_info: HashMap<String, InstalledPackage>,
    ) -> Result<()> {
        if !self.has_hook("PreUninstall") {
            return Ok(());
        }
        let ctx = PreUninstallCtx { main, sdk_info };
        self.call_hook_void("PreUninstall", &ctx)
    }

    // ── Mirror profile discovery ──────────────────────────────────────────────

    /// Read the `PLUGIN.mirrors` table and return all named profiles.
    /// Returns an empty Vec if the plugin does not define any mirrors.
    ///
    /// Expected Lua format (in metadata.lua):
    /// ```lua
    /// PLUGIN = {
    ///   mirrors = {
    ///     { name="default", description="Official",  vars={SDK_FOO_MIRROR="https://..."} },
    ///     { name="china",   description="China CDN", vars={SDK_FOO_MIRROR="https://..."} },
    ///   }
    /// }
    /// ```
    pub fn mirror_profiles(&self) -> Vec<MirrorProfile> {
        let Ok(plugin): Result<LuaTable, _> = self.lua.globals().get(PLUGIN_KEY) else {
            return vec![];
        };
        let Ok(LuaValue::Table(mirrors_tbl)) = plugin.get::<LuaValue>("mirrors") else {
            return vec![];
        };
        let mut profiles = Vec::new();
        for item in mirrors_tbl.sequence_values::<LuaTable>().flatten() {
            let name: String = item.get("name").unwrap_or_default();
            if name.is_empty() { continue; }
            let description: String = item.get("description").unwrap_or_default();
            let mut vars = std::collections::HashMap::new();
            if let Ok(LuaValue::Table(vars_tbl)) = item.get::<LuaValue>("vars") {
                for pair in vars_tbl.pairs::<String, String>() {
                    if let Ok((k, v)) = pair { vars.insert(k, v); }
                }
            }
            profiles.push(MirrorProfile { name, description, vars });
        }
        profiles
    }

    // ── Generic hook dispatch ─────────────────────────────────────────────────

    fn call_hook<C, R>(&self, hook: &str, ctx: &C) -> Result<R>
    where
        C: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let plugin: LuaTable = self.lua.globals().get(PLUGIN_KEY)?;
        let method: LuaFunction = plugin
            .get(hook)
            .with_context(|| format!("hook '{}' not found in plugin", hook))?;

        let ctx_val = self.lua.to_value(ctx)?;
        let result: LuaValue = method
            .call((plugin.clone(), ctx_val))
            .with_context(|| format!("calling plugin hook '{}'", hook))?;

        if matches!(result, LuaValue::Nil) {
            bail!("Plugin hook '{}' returned nil (no result provided)", hook);
        }

        self.lua
            .from_value::<R>(result)
            .with_context(|| format!("deserializing result from hook '{}'", hook))
    }

    fn call_hook_void<C>(&self, hook: &str, ctx: &C) -> Result<()>
    where
        C: Serialize,
    {
        let plugin: LuaTable = self.lua.globals().get(PLUGIN_KEY)?;
        let method: LuaFunction = plugin
            .get(hook)
            .with_context(|| format!("hook '{}' not found", hook))?;
        let ctx_val = self.lua.to_value(ctx)?;
        method
            .call::<()>((plugin.clone(), ctx_val))
            .with_context(|| format!("calling hook '{}'", hook))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lua global & module setup
// ═══════════════════════════════════════════════════════════════════════════════

/// Hook files: (function_name, file_name_without_ext, required)
const HOOK_FILES: &[(&str, &str, bool)] = &[
    ("Available",       "available",          true),
    ("PreInstall",      "pre_install",         true),
    ("EnvKeys",         "env_keys",            true),
    ("PostInstall",     "post_install",        false),
    ("PreUse",          "pre_use",             false),
    ("ParseLegacyFile", "parse_legacy_file",   false),
    ("PreUninstall",    "pre_uninstall",       false),
];

fn setup_globals(lua: &Lua, _plugin_dir: &Path, cfg: &UserConfig) -> Result<()> {
    setup_http_module(lua, cfg)?;
    setup_json_module(lua)?;
    setup_html_module(lua)?;
    setup_string_module(lua)?;
    setup_archiver_module(lua)?;
    setup_os_extensions(lua)?;

    // SDK_NAVIGATOR – used by http module for User-Agent
    let navigator = lua.create_table()?;
    navigator.set(
        "userAgent",
        format!("sdk/{}", env!("CARGO_PKG_VERSION")),
    )?;
    lua.globals().set(NAVIGATOR_KEY, navigator)?;

    // printTable helper
    lua.load(PRELOAD_LUA).set_name("preload.lua").exec()?;

    Ok(())
}

fn limit_package_path(lua: &Lua, paths: &[PathBuf]) -> Result<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let path_str = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(";");
    package.set("path", path_str)?;
    Ok(())
}

// ── http module ───────────────────────────────────────────────────────────────

fn setup_http_module(lua: &Lua, cfg: &UserConfig) -> Result<()> {
    let proxy_url = if cfg.proxy.enable {
        Some(cfg.proxy.url.clone())
    } else {
        None
    };
    let ssl_verify = cfg.proxy.ssl_verify;

    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    preload.set(
        "http",
        lua.create_function(move |lua, ()| {
            let client = crate::util::build_http_client(proxy_url.as_deref(), ssl_verify)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            let client = std::sync::Arc::new(client);

            let tbl = lua.create_table()?;

            // http.get({url, headers}) → (response, error)
            let c = client.clone();
            tbl.set(
                "get",
                lua.create_function(move |lua, opts: LuaTable| {
                    let url: String = opts.get("url").map_err(|_| LuaError::runtime("url is required"))?;
                    // Local file path (local mirror): read file directly instead of HTTP request
                    if !url.starts_with("https://") && !url.starts_with("http://") {
                        return match std::fs::read_to_string(&url) {
                            Ok(body) => {
                                let content_length = body.len() as i64;
                                let r = lua.create_table()?;
                                r.set("status_code", 200i64)?;
                                r.set("body", body)?;
                                r.set("headers", lua.create_table()?)?;
                                r.set("content_length", content_length)?;
                                Ok((LuaValue::Table(r), LuaValue::Nil))
                            }
                            Err(e) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(e.to_string().as_bytes())?))),
                        };
                    }
                    let headers = table_to_headers(&opts);
                    match c.get(&url).headers(headers).send() {
                        Ok(resp) => {
                            let status_code = resp.status().as_u16() as i64;
                            let content_length = resp.content_length().unwrap_or(0) as i64;
                            let resp_headers = build_lua_headers(lua, resp.headers())?;
                            match resp.text() {
                                Ok(body) => {
                                    let r = lua.create_table()?;
                                    r.set("status_code", status_code)?;
                                    r.set("body", body)?;
                                    r.set("headers", resp_headers)?;
                                    r.set("content_length", content_length)?;
                                    Ok((LuaValue::Table(r), LuaValue::Nil))
                                }
                                Err(e) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(e.to_string().as_bytes())?))),
                            }
                        }
                        Err(e) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(e.to_string().as_bytes())?))),
                    }
                })?,
            )?;

            // http.head({url, headers}) → (response, error)
            let c = client.clone();
            tbl.set(
                "head",
                lua.create_function(move |lua, opts: LuaTable| {
                    let url: String = opts.get("url").map_err(|_| LuaError::runtime("url is required"))?;
                    let headers = table_to_headers(&opts);
                    match c.head(&url).headers(headers).send() {
                        Ok(resp) => {
                            let status_code = resp.status().as_u16() as i64;
                            let content_length = resp.content_length().unwrap_or(0) as i64;
                            let resp_headers = build_lua_headers(lua, resp.headers())?;
                            let r = lua.create_table()?;
                            r.set("status_code", status_code)?;
                            r.set("headers", resp_headers)?;
                            r.set("content_length", content_length)?;
                            Ok((LuaValue::Table(r), LuaValue::Nil))
                        }
                        Err(e) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(e.to_string().as_bytes())?))),
                    }
                })?,
            )?;

            // http.download_file({url, headers}, filepath) → error string or nil on success
            let c = client.clone();
            tbl.set(
                "download_file",
                lua.create_function(move |lua, (opts, filepath): (LuaTable, String)| {
                    let url: String = opts.get("url").map_err(|_| LuaError::runtime("url is required"))?;
                    // ensure parent directory exists
                    if let Some(parent) = std::path::Path::new(&filepath).parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            return Ok(LuaValue::String(lua.create_string(e.to_string().as_bytes())?));
                        }
                    }
                    // Local file path (local mirror): copy directly instead of HTTP request
                    if !url.starts_with("https://") && !url.starts_with("http://") {
                        let result: Result<(), String> = std::fs::copy(&url, &filepath)
                            .map(|_| ())
                            .map_err(|e| format!("copying local file '{}': {}", url, e));
                        return match result {
                            Ok(()) => Ok(LuaValue::Nil),
                            Err(e) => {
                                let _ = std::fs::remove_file(&filepath);
                                eprintln!("[sdk] download_file (local) error: {e}");
                                Ok(LuaValue::String(lua.create_string(e.as_bytes())?))
                            }
                        };
                    }
                    let headers = table_to_headers(&opts);
                    let result: Result<(), String> = (|| {
                        let resp = c.get(&url).headers(headers).send()
                            .map_err(|e| e.to_string())?;
                        if !resp.status().is_success() {
                            return Err(format!("HTTP {}", resp.status()));
                        }
                        let total = resp.content_length().unwrap_or(0);
                        let fname = std::path::Path::new(&filepath)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "downloading...".to_string());
                        let pb = indicatif::ProgressBar::new(total);
                        pb.set_style(
                            indicatif::ProgressStyle::with_template(
                                "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                            )
                            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
                            .progress_chars("=>-"),
                        );
                        pb.set_message(fname);
                        let mut source = pb.wrap_read(resp);
                        let mut file = std::fs::File::create(&filepath)
                            .map_err(|e| e.to_string())?;
                        std::io::copy(&mut source, &mut file)
                            .map_err(|e| e.to_string())?;
                        pb.finish_with_message("done");
                        Ok(())
                    })();
                    match result {
                        Ok(()) => Ok(LuaValue::Nil),
                        Err(e) => {
                            // Remove partial/empty file on failure so re-runs don't see a stale file
                            let _ = std::fs::remove_file(&filepath);
                            eprintln!("[sdk] download_file error: {e}");
                            Ok(LuaValue::String(lua.create_string(e.as_bytes())?))
                        }
                    }
                })?,
            )?;

            Ok(tbl)
        })?,
    )?;

    Ok(())
}

fn table_to_headers(tbl: &LuaTable) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    if let Ok(h) = tbl.get::<LuaTable>("headers") {
        for (k, v) in h.pairs::<String, String>().flatten() {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(&v),
            ) {
                map.insert(name, val);
            }
        }
    }
    map
}

fn build_lua_headers(
    lua: &Lua,
    headers: &reqwest::header::HeaderMap,
) -> LuaResult<LuaTable> {
    let tbl = lua.create_table()?;
    for (k, v) in headers {
        if let Ok(v_str) = v.to_str() {
            tbl.set(k.as_str().to_string(), v_str.to_string())?;
        }
    }
    Ok(tbl)
}

// ── json module ───────────────────────────────────────────────────────────────

fn setup_json_module(lua: &Lua) -> Result<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    preload.set(
        "json",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;

            tbl.set(
                "encode",
                lua.create_function(|lua, value: LuaValue| {
                    let json_val: serde_json::Value = lua
                        .from_value(value)
                        .map_err(|e| LuaError::runtime(e.to_string()))?;
                    serde_json::to_string(&json_val)
                        .map_err(|e| LuaError::runtime(e.to_string()))
                })?,
            )?;

            tbl.set(
                "decode",
                lua.create_function(|lua, s: String| {
                    let json_val: serde_json::Value = serde_json::from_str(&s)
                        .map_err(|e: serde_json::Error| LuaError::runtime(e.to_string()))?;
                    lua.to_value(&json_val).map_err(|e: LuaError| LuaError::runtime(e.to_string()))
                })?,
            )?;

            Ok(tbl)
        })?,
    )?;

    Ok(())
}

// ── html module ───────────────────────────────────────────────────────────────

/// Build a Lua selection table from a list of (outer_html, text, inner_html, attrs) tuples.
/// Each selection has :each(fn), :first(), :last(), :eq(n), :attr(k), :text(), :html(), :find(sel).
type ElementData = (String, String, String, Vec<(String, String)>);

fn make_selection(lua: &Lua, elements: Vec<ElementData>) -> LuaResult<LuaTable> {
    let sel = lua.create_table()?;
    let count = elements.len();

    for (i, (outer, text, inner, attrs)) in elements.iter().enumerate() {
        let el = lua.create_table()?;
        el.set("_text", text.clone())?;
        el.set("_html", inner.clone())?;
        el.set("_outer", outer.clone())?;
        let attr_tbl = lua.create_table()?;
        for (k, v) in attrs {
            attr_tbl.set(k.clone(), v.clone())?;
        }
        el.set("_attrs", attr_tbl)?;
        sel.set(i + 1, el)?;
    }
    sel.set("_len", count as i64)?;

    // :each(fn(i, sel)) — passes a single-element selection to the callback
    sel.set("each", lua.create_function(|lua, (tbl, f): (LuaTable, mlua::Function)| {
        let n: i64 = tbl.get("_len").unwrap_or(0);
        for i in 1..=n {
            let el: LuaTable = tbl.get(i)?;
            // Wrap single element in a selection
            let single = make_selection(lua, vec![(
                el.get::<String>("_outer").unwrap_or_default(),
                el.get::<String>("_text").unwrap_or_default(),
                el.get::<String>("_html").unwrap_or_default(),
                {
                    let attr_tbl: LuaTable = el.get("_attrs")?;
                    let mut pairs = Vec::new();
                    for (k, v) in attr_tbl.clone().pairs::<String, String>().flatten() {
                        pairs.push((k, v));
                    }
                    pairs
                },
            )])?;
            f.call::<()>((i, single))?;
        }
        Ok(tbl)
    })?)?;

    // :first() / :last() / :eq(n)
    sel.set("first", lua.create_function(|lua, tbl: LuaTable| {
        let el: mlua::Value = tbl.get(1).unwrap_or(mlua::Value::Nil);
        wrap_single_el(lua, el)
    })?)?;
    sel.set("last", lua.create_function(|lua, tbl: LuaTable| {
        let n: i64 = tbl.get("_len").unwrap_or(0);
        let el: mlua::Value = tbl.get(n).unwrap_or(mlua::Value::Nil);
        wrap_single_el(lua, el)
    })?)?;
    sel.set("eq", lua.create_function(|lua, (tbl, idx): (LuaTable, i64)| {
        let el: mlua::Value = tbl.get(idx).unwrap_or(mlua::Value::Nil);
        wrap_single_el(lua, el)
    })?)?;

    // :attr(name) — on first element
    sel.set("attr", lua.create_function(|_, (tbl, name): (LuaTable, String)| {
        let el: mlua::Result<LuaTable> = tbl.get(1);
        if let Ok(el) = el {
            let attrs: LuaTable = el.get("_attrs")?;
            let val: mlua::Value = attrs.get(name)?;
            return Ok(val);
        }
        Ok(mlua::Value::Nil)
    })?)?;

    // :text() — first element's text
    sel.set("text", lua.create_function(|_, tbl: LuaTable| {
        let el: mlua::Result<LuaTable> = tbl.get(1);
        Ok(el.ok().and_then(|e| e.get::<String>("_text").ok()).unwrap_or_default())
    })?)?;

    // :html() — first element's inner html
    sel.set("html", lua.create_function(|_, tbl: LuaTable| {
        let el: mlua::Result<LuaTable> = tbl.get(1);
        Ok(el.ok().and_then(|e| e.get::<String>("_html").ok()).unwrap_or_default())
    })?)?;

    // :find(selector) — re-parse outer html of each element and search
    sel.set("find", lua.create_function(|lua, (tbl, selector): (LuaTable, String)| {
        let n: i64 = tbl.get("_len").unwrap_or(0);
        let mut found: Vec<ElementData> = Vec::new();
        for i in 1..=n {
            let el: mlua::Result<LuaTable> = tbl.get(i);
            if let Ok(el) = el {
                let outer: String = el.get("_outer").unwrap_or_default();
                let doc = scraper::Html::parse_fragment(&outer);
                if let Ok(css) = scraper::Selector::parse(&selector) {
                    for matched in doc.select(&css) {
                        found.push(extract_element(&matched));
                    }
                }
            }
        }
        make_selection(lua, found)
    })?)?;

    Ok(sel)
}

fn extract_element(el: &scraper::ElementRef) -> (String, String, String, Vec<(String,String)>) {
    let outer = el.html();
    let text  = el.text().collect::<String>();
    let inner = el.inner_html();
    let attrs = el.value().attrs().map(|(k,v)| (k.to_string(), v.to_string())).collect();
    (outer, text, inner, attrs)
}

fn wrap_single_el(lua: &Lua, el: mlua::Value) -> LuaResult<LuaTable> {
    match el {
        mlua::Value::Table(t) => {
            let attrs: LuaTable = t.get("_attrs")?;
            let mut pairs = Vec::new();
            for (k, v) in attrs.pairs::<String, String>().flatten() {
                pairs.push((k, v));
            }
            make_selection(lua, vec![(
                t.get::<String>("_outer").unwrap_or_default(),
                t.get::<String>("_text").unwrap_or_default(),
                t.get::<String>("_html").unwrap_or_default(),
                pairs,
            )])
        }
        _ => make_selection(lua, vec![]),
    }
}

fn setup_html_module(lua: &Lua) -> Result<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    preload.set(
        "html",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;
            tbl.set(
                "parse",
                lua.create_function(|lua, html_str: String| {
                    let doc = scraper::Html::parse_document(&html_str);
                    // Root "document" selection — find all top-level elements
                    let root_sel = scraper::Selector::parse("*").unwrap();
                    let elements: Vec<_> = doc.select(&root_sel).map(|el| extract_element(&el)).collect();
                    let sel = make_selection(lua, elements)?;
                    // Override :find() to search the full document html
                    sel.set("find", lua.create_function(move |lua, (_tbl, selector): (LuaTable, String)| {
                        let doc2 = scraper::Html::parse_document(&html_str);
                        let css = scraper::Selector::parse(&selector)
                            .map_err(|e| LuaError::runtime(format!("{:?}", e)))?;
                        let found: Vec<_> = doc2.select(&css).map(|el| extract_element(&el)).collect();
                        make_selection(lua, found)
                    })?)?;
                    Ok(sel)
                })?,
            )?;
            Ok(tbl)
        })?,
    )?;

    Ok(())
}

// ── string module (extra functions) ──────────────────────────────────────────

fn setup_string_module(lua: &Lua) -> Result<()> {
    // Extend the standard `string` table with trim/split (for compatibility)
    let string_tbl: LuaTable = lua.globals().get("string")?;
    string_tbl.set(
        "trim",
        lua.create_function(|_, s: String| Ok(s.trim().to_string()))?,
    )?;
    string_tbl.set(
        "split",
        lua.create_function(|lua, (s, sep): (String, String)| {
            let parts: Vec<_> = s.split(sep.as_str()).map(|p| p.to_string()).collect();
            let tbl = lua.create_table()?;
            for (i, p) in parts.iter().enumerate() {
                tbl.set(i + 1, p.clone())?;
            }
            Ok(tbl)
        })?,
    )?;

    // Register `sdk.strings` as a preloaded module
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;
    preload.set(
        "sdk.strings",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;

            tbl.set("split", lua.create_function(|lua, (s, sep): (String, mlua::Value)| {
                let sep_str: String = match sep {
                    mlua::Value::String(ref ls) => ls.to_str()?.to_string(),
                    _ => String::new(),
                };
                let parts: Vec<_> = if sep_str.is_empty() {
                    s.split_whitespace().map(|p| p.to_string()).collect()
                } else {
                    s.split(sep_str.as_str()).map(|p| p.to_string()).collect()
                };
                let tbl = lua.create_table()?;
                for (i, p) in parts.iter().enumerate() {
                    tbl.set(i + 1, p.clone())?;
                }
                Ok(tbl)
            })?)?;

            tbl.set("trim", lua.create_function(|_, (s, cutset): (String, String)| {
                let chars: Vec<char> = cutset.chars().collect();
                Ok(s.trim_matches(chars.as_slice()).to_string())
            })?)?;

            tbl.set("trim_space", lua.create_function(|_, s: String| {
                Ok(s.trim().to_string())
            })?)?;

            tbl.set("trim_prefix", lua.create_function(|_, (s, prefix): (String, String)| {
                Ok(s.strip_prefix(prefix.as_str()).unwrap_or(&s).to_string())
            })?)?;

            tbl.set("trim_suffix", lua.create_function(|_, (s, suffix): (String, String)| {
                Ok(s.strip_suffix(suffix.as_str()).unwrap_or(&s).to_string())
            })?)?;

            tbl.set("has_prefix", lua.create_function(|_, (s, prefix): (String, String)| {
                Ok(s.starts_with(prefix.as_str()))
            })?)?;

            tbl.set("has_suffix", lua.create_function(|_, (s, suffix): (String, String)| {
                Ok(s.ends_with(suffix.as_str()))
            })?)?;

            tbl.set("contains", lua.create_function(|_, (s, sub): (String, String)| {
                Ok(s.contains(sub.as_str()))
            })?)?;

            tbl.set("fields", lua.create_function(|lua, s: String| {
                let parts: Vec<_> = s.split_whitespace().map(|p| p.to_string()).collect();
                let tbl = lua.create_table()?;
                for (i, p) in parts.iter().enumerate() {
                    tbl.set(i + 1, p.clone())?;
                }
                Ok(tbl)
            })?)?;

            tbl.set("join", lua.create_function(|_, (arr, sep): (mlua::Table, String)| {
                let mut parts: Vec<String> = Vec::new();
                let len = arr.len()?;
                for i in 1..=len {
                    let v: mlua::Value = arr.get(i)?;
                    parts.push(v.to_string()?);
                }
                Ok(parts.join(&sep))
            })?)?;

            Ok(tbl)
        })?,
    )?;

    Ok(())
}

// ── archiver module ───────────────────────────────────────────────────────────

fn setup_archiver_module(lua: &Lua) -> Result<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    preload.set(
        "archiver",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;
            // archiver.unarchive(src, dest) → error
            tbl.set(
                "unarchive",
                lua.create_function(|lua, (src, dest): (String, String)| {
                    match crate::util::extract(&src, &dest) {
                        Ok(_) => Ok(LuaValue::Nil),
                        Err(e) => Ok(LuaValue::String(lua.create_string(e.to_string().as_bytes())?)),
                    }
                })?,
            )?;
            Ok(tbl)
        })?,
    )?;

    Ok(())
}

// ── os extensions ─────────────────────────────────────────────────────────────

/// Add sdk-specific extensions to the standard Lua `os` table:
/// - os.setenv(key, value)  — set a process env variable
/// - os.unsetenv(key)       — remove a process env variable
///
/// Also overrides os.getenv to check `__SDK_PLUGIN_ENV` table first.
const PLUGIN_ENV_KEY: &str = "__SDK_PLUGIN_ENV";

fn setup_os_extensions(lua: &Lua) -> Result<()> {
    // Install pure-Lua overrides for os.getenv and os.execute.
    // Using Lua code is more reliable than Rust closures for upvalue capture.
    lua.load(r#"
-- os.getenv: check plugin-local env table first, fall back to real env
local _orig_os_getenv = os.getenv
__SDK_PLUGIN_ENV = {}
function os.getenv(key)
    local v = __SDK_PLUGIN_ENV[key]
    if v ~= nil then return v end
    return _orig_os_getenv(key)
end

-- os.execute: Lua 5.1 compat shim — sdk plugins expect integer exit code.
-- Lua 5.4 returns (true|nil, "exit"|"signal", code); we normalise to integer.
local _orig_os_execute = os.execute
function os.execute(cmd)
    if cmd == nil then return _orig_os_execute() end
    local ok, _, code = _orig_os_execute(cmd)
    if ok then return 0 else return code or 1 end
end
"#)
    .set_name("os_extensions")
    .exec()?;

    let os_tbl: LuaTable = lua.globals().get("os")?;

    os_tbl.set(
        "setenv",
        lua.create_function(|_, (key, val): (String, mlua::Value)| {
            let val_str = match &val {
                mlua::Value::String(s) => s.to_str()?.to_string(),
                mlua::Value::Integer(n) => n.to_string(),
                mlua::Value::Number(n)  => n.to_string(),
                mlua::Value::Boolean(b) => b.to_string(),
                mlua::Value::Nil        => { std::env::remove_var(&key); return Ok(()); }
                other => other.to_string()?,
            };
            std::env::set_var(&key, val_str);
            Ok(())
        })?,
    )?;

    os_tbl.set(
        "unsetenv",
        lua.create_function(|_, key: String| {
            std::env::remove_var(&key);
            Ok(())
        })?,
    )?;

    Ok(())
}

/// Inject plugin-specific defaults into the plugin-local env table (__SDK_PLUGIN_ENV).
/// Called after the plugin name is known, before any hooks are invoked.
fn inject_plugin_env_defaults(lua: &Lua, plugin_name: &str) -> Result<()> {
    // Python on Windows: default to UV build (prebuilt binaries) unless the user explicitly
    // opts out.  The alternative path (WiX dark.exe + msiexec) requires a fully functional
    // Windows Installer infrastructure (C:\Windows\Installer) which is often absent in
    // container / minimal environments.  UV build is more portable and works out of the box.
    #[cfg(windows)]
    {
        if plugin_name == "python"
            && std::env::var("SDK_PYTHON_USE_UV_BUILD").is_err()
        {
            let win_installer = std::path::Path::new("C:\\Windows\\Installer");
            if !win_installer.exists() {
                // Windows Installer infrastructure not available; use UV prebuilt binaries
                let plugin_env: LuaTable = lua.globals().get(PLUGIN_ENV_KEY)?;
                plugin_env.set("SDK_PYTHON_USE_UV_BUILD", "true")?;
            }
            // If C:\Windows\Installer exists, WiX + msiexec path is assumed to work
        }
    }
    // suppress unused-variable warnings on non-Windows
    #[cfg(not(windows))]
    let _ = (lua, plugin_name);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Utilities
// ═══════════════════════════════════════════════════════════════════════════════

fn is_no_result(e: &anyhow::Error) -> bool {
    e.to_string().contains("returned nil")
        || e.to_string().contains("no result provided")
}

fn get_os_type() -> String {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos"   => "darwin",
        "linux"   => "linux",
        other     => other,
    }
    .to_string()
}

fn get_arch_type() -> String {
    match std::env::consts::ARCH {
        "x86_64"  => "amd64",
        "aarch64" => "arm64",
        "arm"     => "arm",
        "x86"     => "386",
        other     => other,
    }
    .to_string()
}

const PRELOAD_LUA: &str = r#"
function printTable(t, indent)
    indent = indent or 0
    local strIndent = string.rep("  ", indent)
    for key, value in pairs(t) do
        local keyStr = tostring(key)
        if type(value) == "table" then
            print(strIndent .. "[" .. keyStr .. "] =>")
            printTable(value, indent + 1)
        else
            print(strIndent .. "[" .. keyStr .. "] => " .. tostring(value))
        end
    end
end
"#;
