local http = require("http")
local json = require("json")

-- ── 配置（从环境变量读取，支持 mirror 切换）────────────────────────────────
local UV_BIN          = os.getenv("SDK_UV_BIN")                or "uv"
local STANDALONE_BASE = os.getenv("SDK_PYTHON_STANDALONE_MIRROR")
                        or "https://github.com/astral-sh/python-build-standalone/releases/download"
local STANDALONE_API  = os.getenv("SDK_PYTHON_STANDALONE_API") or ""
local STANDALONE_TAG  = os.getenv("SDK_PYTHON_STANDALONE_TAG") or ""

local GITHUB_ATOM = "https://github.com/astral-sh/python-build-standalone/releases.atom"

-- ── uv 路径 ──────────────────────────────────────────────────────────────────

local function uv_available()
    local devnull = OS_TYPE == "windows" and "nul" or "/dev/null"
    local ret = os.execute(string.format('"%s" --version >%s 2>&1', UV_BIN, devnull))
    return ret == true or ret == 0
end

-- 通过 uv python list 获取指定版本的下载 URL
-- 返回 URL 字符串，或 nil（版本已安装 / uv 不支持）
local function uv_get_url(version)
    local cmd = string.format('"%s" python list --all-versions --output-format json', UV_BIN)
    local handle = io.popen(cmd, "r")
    if not handle then return nil end
    local output = handle:read("*a")
    handle:close()
    local ok, data = pcall(json.decode, output)
    if not ok or type(data) ~= "table" then return nil end
    for _, item in ipairs(data) do
        -- 注意：JSON null 在 mlua 中为 userdata，需用 type() 判断
        if item.implementation == "cpython"
            and item.version == version
            and type(item.url) == "string" and item.url ~= ""
            and (type(item.path) ~= "string" or item.path == "")
            and (item.variant == "default" or item.variant == nil or type(item.variant) ~= "string")
        then
            return item.url
        end
    end
    return nil
end

-- ── 直接下载路径（python-build-standalone）──────────────────────────────────

local function is_flat(path)
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_PYTHON_FLAT") == "1"
end

local function is_npmmirror()
    return STANDALONE_API ~= "" and STANDALONE_API:find("npmmirror") ~= nil
end

local function get_latest_tag()
    if STANDALONE_TAG ~= "" then return STANDALONE_TAG end

    if is_npmmirror() then
        local resp, err = http.get({ url = STANDALONE_API .. "/" })
        if err ~= nil then error("npmmirror API error: " .. tostring(err)) end
        if resp.status_code ~= 200 then error("npmmirror API HTTP " .. resp.status_code) end
        local data = json.decode(resp.body)
        local latest = ""
        for _, item in ipairs(data) do
            if item.type == "dir" then
                local tag = item.name:gsub("/$", "")
                if #tag == 8 and tag:match("^%d+$") and tag > latest then
                    latest = tag
                end
            end
        end
        if latest == "" then error("No standalone releases found on npmmirror") end
        return latest
    else
        local resp, err = http.get({ url = GITHUB_ATOM })
        if err ~= nil then error("GitHub atom error: " .. tostring(err)) end
        if resp.status_code ~= 200 then error("GitHub atom HTTP " .. resp.status_code) end
        local tag = resp.body:match("Repository/%d+/(%d%d%d%d%d%d%d%d)")
        if not tag then error("Could not parse latest tag from GitHub atom feed") end
        return tag
    end
end

local function get_platform()
    local os_type = OS_TYPE
    local arch    = ARCH_TYPE
    if os_type == "linux" then
        return (arch == "arm64") and "aarch64-unknown-linux-gnu" or "x86_64-unknown-linux-gnu"
    elseif os_type == "darwin" then
        return (arch == "arm64") and "aarch64-apple-darwin" or "x86_64-apple-darwin"
    elseif os_type == "windows" then
        return "x86_64-pc-windows-msvc"
    else
        error("Unsupported OS: " .. tostring(os_type))
    end
end

local function direct_get_url(version)
    local platform = get_platform()
    if is_flat(STANDALONE_BASE) then
        local filename = string.format("cpython-%s+latest-%s-install_only.tar.gz", version, platform)
        return STANDALONE_BASE .. "/" .. filename
    end
    local tag      = get_latest_tag()
    local filename = string.format("cpython-%s+%s-%s-install_only.tar.gz", version, tag, platform)
    return STANDALONE_BASE .. "/" .. tag .. "/" .. filename
end

-- ── 入口 ─────────────────────────────────────────────────────────────────────

function PLUGIN:PreInstall(ctx)
    local version = ctx.version

    -- 优先通过 uv 获取下载 URL（uv 负责镜像路由，含 UV_PYTHON_INSTALL_MIRROR 支持）
    if uv_available() then
        local url = uv_get_url(version)
        if url then
            return { version = version, url = url }
        end
        -- uv 未能提供 URL（版本已被其他工具安装，uv 隐藏了其下载地址），
        -- 自动降级：直接从 python-build-standalone 计算 URL
    end

    -- 直接计算 python-build-standalone 下载 URL
    return { version = version, url = direct_get_url(version) }
end
