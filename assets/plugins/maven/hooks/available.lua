local http = require("http")
local html = require("html")
local json = require("json")

-- Maven release index
-- SDK_MAVEN_MIRROR is the base without major version, e.g. "https://dlcdn.apache.org/maven"
-- For flat/local mirrors it is the directory containing versions.json
local MIRROR_BASE = os.getenv("SDK_MAVEN_MIRROR") or "https://dlcdn.apache.org/maven"
local ARCHIVE_BASE = "https://archive.apache.org/dist/maven"

local function is_flat(path)
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_MAVEN_FLAT") == "1"
end

-- Return versioned repo URL: <base>/maven-<major>
local function repo_url(base, major)
    return string.format("%s/maven-%s", base, major)
end

-- Fetch version list from an Apache-style directory listing
local function fetch_versions_from_dir(url)
    local resp, err = http.get({ url = url .. "/" })
    if err ~= nil then return nil, err end
    if resp.status_code ~= 200 then
        return nil, "HTTP " .. resp.status_code
    end
    local doc = html.parse(resp.body)
    local versions = {}
    local seen = {}
    doc:find("a"):each(function(_, el)
        local href = el:attr("href")
        if href then
            local ver = href:match("^(%d+%.%d+%.%d+)/?$")
            if ver and not seen[ver] then
                seen[ver] = true
                table.insert(versions, ver)
            end
        end
    end)
    return versions, nil
end

function PLUGIN:Available(ctx)
    local result = {}

    if is_flat(MIRROR_BASE) then
        -- Local mirror: read versions.json (simple array of version strings)
        local resp, err = http.get({ url = MIRROR_BASE .. "/versions.json" })
        if err ~= nil then
            error("Failed to read local Maven versions: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("versions.json not found at " .. MIRROR_BASE)
        end
        local data = json.decode(resp.body)
        for _, ver in ipairs(data) do
            table.insert(result, { version = tostring(ver) })
        end
        return result
    end

    -- Collect versions from Maven 3 and Maven 4 directories
    -- Try current mirror first; fall back to Apache Archive for missing versions
    local majors = { "3", "4" }
    local seen_all = {}

    for _, major in ipairs(majors) do
        local url = repo_url(MIRROR_BASE, major)
        local versions, err = fetch_versions_from_dir(url)
        if err ~= nil or not versions or #versions == 0 then
            -- Fall back to Apache Archive
            url = repo_url(ARCHIVE_BASE, major)
            versions, err = fetch_versions_from_dir(url)
        end
        if versions then
            for _, ver in ipairs(versions) do
                if not seen_all[ver] then
                    seen_all[ver] = true
                    table.insert(result, { version = ver })
                end
            end
        end
    end

    -- Sort descending (numeric: major.minor.patch)
    local function parse_ver(v)
        local a, b, c = v:match("^(%d+)%.(%d+)%.(%d+)")
        return tonumber(a) or 0, tonumber(b) or 0, tonumber(c) or 0
    end
    table.sort(result, function(a, b)
        local ma, mib, pa = parse_ver(a.version)
        local mb, mib2, pb = parse_ver(b.version)
        if ma ~= mb then return ma > mb end
        if mib ~= mib2 then return mib > mib2 end
        return pa > pb
    end)

    return result
end
