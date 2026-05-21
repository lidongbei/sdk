use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use mlua::{
    Error as LuaError,
    Function as LuaFunction,
    Lua,
    LuaSerdeExt,
    Result as LuaResult,
    Table as LuaTable,
    Value as LuaValue,
};
use serde::{Deserialize, Serialize};

use crate::config::UserConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// Hook data models (mirror vfox's plugin/model.go)
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

#[derive(Debug, Serialize)]
pub struct PreInstallCtx {
    pub version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PreInstallResult {
    pub name:     Option<String>,
    pub version:  String,
    /// Download URL or local file path.  Empty means no download needed.
    #[serde(rename = "url", default)]
    pub url:      String,
    #[serde(default)]
    pub headers:  HashMap<String, String>,
    #[serde(default)]
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
const NAVIGATOR_KEY: &str = "VFOX_NAVIGATOR";

pub struct LuaPlugin {
    lua:          Lua,
    pub metadata: PluginMetadata,
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
            .context("PLUGIN global not found – is this a valid vfox plugin?")?;

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

    // VFOX_NAVIGATOR – used by http module for User-Agent
    let navigator = lua.create_table()?;
    navigator.set(
        "userAgent",
        format!("vfox/{}", env!("CARGO_PKG_VERSION")),
    )?;
    lua.globals().set(NAVIGATOR_KEY, navigator)?;

    // printTable helper (included in vfox's preload.lua)
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

            // http.download_file({url, headers}, filepath) → error
            let c = client.clone();
            tbl.set(
                "download_file",
                lua.create_function(move |lua, (opts, filepath): (LuaTable, String)| {
                    let url: String = opts.get("url").map_err(|_| LuaError::runtime("url is required"))?;
                    let headers = table_to_headers(&opts);
                    match c.get(&url).headers(headers).send() {
                        Ok(resp) if resp.status().is_success() => {
                            match resp.bytes() {
                                Ok(bytes) => {
                                    if let Err(e) = std::fs::write(&filepath, &bytes) {
                                        return Ok(LuaValue::String(lua.create_string(e.to_string().as_bytes())?));
                                    }
                                    Ok(LuaValue::Nil)
                                }
                                Err(e) => Ok(LuaValue::String(lua.create_string(e.to_string().as_bytes())?)),
                            }
                        }
                        Ok(resp) => Ok(LuaValue::String(lua.create_string(
                            format!("HTTP {}", resp.status()).as_bytes(),
                        )?)),
                        Err(e) => Ok(LuaValue::String(lua.create_string(e.to_string().as_bytes())?)),
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
        for pair in h.pairs::<String, String>() {
            if let Ok((k, v)) = pair {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(&v),
                ) {
                    map.insert(name, val);
                }
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

fn setup_html_module(lua: &Lua) -> Result<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    preload.set(
        "html",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;

            // html.parse(html_string) → document with :find(selector) method
            tbl.set(
                "parse",
                lua.create_function(|lua, html: String| {
                    let doc = scraper::Html::parse_document(&html);
                    // Expose .find(selector) → list of {text, inner_html, href, ...}
                    let doc_tbl = lua.create_table()?;

                    // Serialize the document nodes for Lua access
                    let nodes: Vec<_> = doc
                        .select(&scraper::Selector::parse("*").unwrap())
                        .map(|el| {
                            let t = lua.create_table().unwrap();
                            t.set("text", el.text().collect::<String>()).unwrap();
                            t.set("inner_html", el.inner_html()).unwrap();
                            if let Some(href) = el.value().attr("href") {
                                t.set("href", href.to_string()).unwrap();
                            }
                            LuaValue::Table(t)
                        })
                        .collect();

                    for (i, node) in nodes.into_iter().enumerate() {
                        doc_tbl.set(i + 1, node)?;
                    }

                    // Add a .find(selector) method
                    let html_clone = html.clone();
                    doc_tbl.set(
                        "find",
                        lua.create_function(move |lua, selector: String| {
                            let doc = scraper::Html::parse_document(&html_clone);
                            let sel = scraper::Selector::parse(&selector)
                                .map_err(|e| LuaError::runtime(format!("CSS selector error: {:?}", e)))?;
                            let result = lua.create_table()?;
                            for (i, el) in doc.select(&sel).enumerate() {
                                let t = lua.create_table()?;
                                t.set("text", el.text().collect::<String>())?;
                                t.set("inner_html", el.inner_html())?;
                                if let Some(href) = el.value().attr("href") {
                                    t.set("href", href.to_string())?;
                                }
                                for attr_name in &["class", "id", "src", "data-version", "title"] {
                                    if let Some(val) = el.value().attr(attr_name) {
                                        t.set(*attr_name, val.to_string())?;
                                    }
                                }
                                result.set(i + 1, t)?;
                            }
                            Ok(result)
                        })?,
                    )?;

                    Ok(doc_tbl)
                })?,
            )?;
            Ok(tbl)
        })?,
    )?;

    Ok(())
}

// ── string module (extra functions) ──────────────────────────────────────────

fn setup_string_module(lua: &Lua) -> Result<()> {
    // Additional string utilities on the existing `string` table
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
