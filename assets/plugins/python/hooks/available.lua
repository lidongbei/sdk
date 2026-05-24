local http = require("http")
local json = require("json")

local BASE_URL        = os.getenv("SDK_PYTHON_MIRROR")            or "https://www.python.org"
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
        -- GitHub releases.atom: parse first tag from <id>Repository/NNN/YYYYMMDD</id>
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

-- Fetch installable Python versions for the current platform from a standalone release
local function get_standalone_versions(tag)
    local platform = get_platform()
    -- Lua pattern: match install_only tarball, not stripped/freethreaded variants
    local pat = "^cpython%-([%d%.]+)%+" .. tag .. "%-" .. platform:gsub("%-", "%%-") .. "%-install_only%.tar%.gz$"

    local filenames = {}

    if is_npmmirror() then
        local resp, err = http.get({ url = STANDALONE_API .. "/" .. tag .. "/" })
        if err ~= nil then error("npmmirror files error: " .. tostring(err)) end
        if resp.status_code ~= 200 then error("npmmirror files HTTP " .. resp.status_code) end
        local data = json.decode(resp.body)
        for _, item in ipairs(data) do
            if item.type == "file" and item.name then
                table.insert(filenames, item.name)
            end
        end
    else
        -- GitHub expanded_assets HTML: extract filenames from download hrefs
        local url = "https://github.com/astral-sh/python-build-standalone/releases/expanded_assets/" .. tag
        local resp, err = http.get({ url = url })
        if err ~= nil then error("GitHub expanded_assets error: " .. tostring(err)) end
        if resp.status_code ~= 200 then error("GitHub expanded_assets HTTP " .. resp.status_code) end
        for filename in resp.body:gmatch('/releases/download/[^/]+/([^"]+)') do
            table.insert(filenames, filename)
        end
    end

    local versions = {}
    local seen = {}
    for _, name in ipairs(filenames) do
        local ver = name:match(pat)
        if ver and not seen[ver] then
            seen[ver] = true
            table.insert(versions, ver)
        end
    end

    -- Sort descending by version number
    table.sort(versions, function(a, b)
        local a1, a2, a3 = a:match("(%d+)%.(%d+)%.(%d+)")
        local b1, b2, b3 = b:match("(%d+)%.(%d+)%.(%d+)")
        if a1 ~= b1 then return tonumber(a1) > tonumber(b1) end
        if a2 ~= b2 then return tonumber(a2) > tonumber(b2) end
        return tonumber(a3) > tonumber(b3)
    end)

    return versions
end

function PLUGIN:Available(ctx)
    if is_flat(BASE_URL) then
        -- Local mirror: read versions.json (simple array of version strings)
        local resp, err = http.get({ url = BASE_URL .. "/versions.json" })
        if err ~= nil then
            error("Failed to read local Python versions: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("versions.json not found at " .. BASE_URL)
        end
        local data = json.decode(resp.body)
        local result = {}
        for _, ver in ipairs(data) do
            table.insert(result, { version = tostring(ver) })
        end
        return result
    end

    -- Online: use python-build-standalone releases for accurate version list
    local tag = get_latest_tag()
    local versions = get_standalone_versions(tag)

    local result = {}
    for _, ver in ipairs(versions) do
        table.insert(result, { version = ver })
    end
    return result
end
