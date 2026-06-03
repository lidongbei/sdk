local http = require("http")
local json = require("json")

local BASE_URL        = os.getenv("SDK_PYTHON_MIRROR")            or "https://www.python.org"
local STANDALONE_API  = os.getenv("SDK_PYTHON_STANDALONE_API")    or ""
local STANDALONE_TAG  = os.getenv("SDK_PYTHON_STANDALONE_TAG")    or ""

local GITHUB_RELEASES_API = "https://api.github.com/repos/astral-sh/python-build-standalone/releases"

local function is_flat(path)
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_PYTHON_FLAT") == "1"
end

local function is_npmmirror()
    return STANDALONE_API ~= "" and STANDALONE_API:find("npmmirror") ~= nil
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

-- Sort version strings descending (e.g. "3.12.5" > "3.12.0" > "3.11.9")
local function sort_versions_desc(versions)
    table.sort(versions, function(a, b)
        local a1, a2, a3 = a:match("(%d+)%.(%d+)%.(%d+)")
        local b1, b2, b3 = b:match("(%d+)%.(%d+)%.(%d+)")
        if not a1 or not b1 then return a > b end
        if a1 ~= b1 then return tonumber(a1) > tonumber(b1) end
        if a2 ~= b2 then return tonumber(a2) > tonumber(b2) end
        return tonumber(a3 or 0) > tonumber(b3 or 0)
    end)
end

-- Fetch Python versions from ALL python-build-standalone GitHub releases.
-- Uses the releases API (assets are included in response), paginating through
-- up to 10 pages (1000 releases) to cover the full release history.
local function get_all_github_versions()
    local platform = get_platform()
    local versions = {}
    local seen = {}
    local page = 1
    local max_pages = 10  -- 1000 releases max; python-build-standalone has ~50

    while page <= max_pages do
        local url = GITHUB_RELEASES_API .. "?per_page=100&page=" .. page
        local resp, err = http.get({
            url = url,
            headers = { Accept = "application/vnd.github+json" }
        })
        if err ~= nil or resp.status_code ~= 200 then break end
        local releases = json.decode(resp.body)
        if type(releases) ~= "table" or #releases == 0 then break end

        for _, release in ipairs(releases) do
            local tag = release.tag_name
            -- Lua pattern: match install_only tarball, not stripped/freethreaded variants
            local pat = "^cpython%-([%d%.]+)%+" .. tag .. "%-" .. platform:gsub("%-", "%%-") .. "%-install_only%.tar%.gz$"

            for _, asset in ipairs(release.assets or {}) do
                local ver = asset.name:match(pat)
                if ver and not seen[ver] then
                    seen[ver] = true
                    table.insert(versions, ver)
                end
            end
        end

        -- If fewer than 100 releases returned, we've reached the end
        if #releases < 100 then break end
        page = page + 1
    end

    sort_versions_desc(versions)
    return versions
end

-- Fetch Python versions from ALL npmmirror tags.
-- Iterates over every YYYYMMDD tag directory to collect versions across all releases.
local function get_all_npmmirror_versions()
    local platform = get_platform()
    local versions = {}
    local seen = {}

    -- List all tag directories
    local resp, err = http.get({ url = STANDALONE_API .. "/" })
    if err ~= nil then error("npmmirror API error: " .. tostring(err)) end
    if resp.status_code ~= 200 then error("npmmirror API HTTP " .. resp.status_code) end
    local data = json.decode(resp.body)

    local tags = {}
    for _, item in ipairs(data) do
        if item.type == "dir" then
            local tag = item.name:gsub("/$", "")
            if #tag == 8 and tag:match("^%d+$") then
                table.insert(tags, tag)
            end
        end
    end

    -- Sort newest first
    table.sort(tags, function(a, b) return a > b end)

    -- Collect versions from each tag
    for _, tag in ipairs(tags) do
        local pat = "^cpython%-([%d%.]+)%+" .. tag .. "%-" .. platform:gsub("%-", "%%-") .. "%-install_only%.tar%.gz$"

        local fresp, ferr = http.get({ url = STANDALONE_API .. "/" .. tag .. "/" })
        if ferr == nil and fresp.status_code == 200 then
            local files = json.decode(fresp.body)
            for _, item in ipairs(files) do
                if item.type == "file" and item.name then
                    local ver = item.name:match(pat)
                    if ver and not seen[ver] then
                        seen[ver] = true
                        table.insert(versions, ver)
                    end
                end
            end
        end
    end

    sort_versions_desc(versions)
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

    -- Aggregate versions from ALL python-build-standalone releases
    local versions
    if is_npmmirror() then
        versions = get_all_npmmirror_versions()
    elseif STANDALONE_TAG ~= "" then
        -- Specific tag override: only show versions from that one release
        local platform = get_platform()
        local tag = STANDALONE_TAG
        local pat = "^cpython%-([%d%.]+)%+" .. tag .. "%-" .. platform:gsub("%-", "%%-") .. "%-install_only%.tar%.gz$"
        local filenames = {}
        local url = "https://github.com/astral-sh/python-build-standalone/releases/expanded_assets/" .. tag
        local resp, err = http.get({ url = url })
        if err ~= nil then error("GitHub expanded_assets error: " .. tostring(err)) end
        if resp.status_code ~= 200 then error("GitHub expanded_assets HTTP " .. resp.status_code) end
        for filename in resp.body:gmatch('/releases/download/[^/]+/([^"]+)') do
            table.insert(filenames, filename)
        end
        versions = {}
        local seen = {}
        for _, name in ipairs(filenames) do
            local ver = name:match(pat)
            if ver and not seen[ver] then
                seen[ver] = true
                table.insert(versions, ver)
            end
        end
        sort_versions_desc(versions)
    else
        -- Default: aggregate across all GitHub releases
        versions = get_all_github_versions()
    end

    local result = {}
    for _, ver in ipairs(versions) do
        table.insert(result, { version = ver })
    end
    return result
end
