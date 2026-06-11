local http = require("http")
local json = require("json")

local BASE_URL        = os.getenv("SDK_PYTHON_MIRROR")            or "https://www.python.org"
local STANDALONE_BASE = os.getenv("SDK_PYTHON_STANDALONE_MIRROR") or "https://github.com/astral-sh/python-build-standalone/releases/download"
local STANDALONE_API  = os.getenv("SDK_PYTHON_STANDALONE_API")    or ""
local STANDALONE_TAG  = os.getenv("SDK_PYTHON_STANDALONE_TAG")    or ""

local GITHUB_ATOM = "https://github.com/astral-sh/python-build-standalone/releases.atom"

local function is_flat(path)
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_PYTHON_FLAT") == "1"
end

local function is_npmmirror()
    return STANDALONE_API ~= "" and STANDALONE_API:find("npmmirror") ~= nil
end

-- Get the latest standalone release tag (YYYYMMDD)
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

-- Build the platform string used in standalone filenames
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

function PLUGIN:PreInstall(ctx)
    local version = ctx.version

    -- Flat/local mirror: construct filename directly without API calls
    if is_flat(STANDALONE_BASE) then
        local platform = get_platform()
        local filename = string.format("cpython-%s+latest-%s-install_only.tar.gz", version, platform)
        return { version = version, url = STANDALONE_BASE .. "/" .. filename }
    end

    -- Online: dynamically resolve the latest tag and build download URL
    local tag      = get_latest_tag()
    local platform = get_platform()
    local filename = string.format("cpython-%s+%s-%s-install_only.tar.gz", version, tag, platform)
    local url      = STANDALONE_BASE .. "/" .. tag .. "/" .. filename

    return { version = version, url = url }
end
