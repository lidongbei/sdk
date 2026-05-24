local http = require("http")
local json = require("json")

-- Gradle releases API
-- Online: SDK_GRADLE_API = "https://services.gradle.org/versions/all"
-- Local:  SDK_GRADLE_API = "/path/to/mirror/gradle"  (reads versions.json)
local API_URL = os.getenv("SDK_GRADLE_API") or "https://services.gradle.org/versions/all"

local function is_local(url)
    return url:sub(1, 4) ~= "http" or os.getenv("SDK_FLAT_MIRROR") == "1"
end

function PLUGIN:Available(ctx)
    local result = {}

    if is_local(API_URL) then
        -- Local mirror: read versions.json (simple array of version strings)
        local resp, err = http.get({ url = API_URL .. "/versions.json" })
        if err ~= nil then
            error("Failed to read local Gradle versions: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("versions.json not found at " .. API_URL)
        end
        local data = json.decode(resp.body)
        for _, ver in ipairs(data) do
            table.insert(result, { version = tostring(ver) })
        end
        return result
    end

    -- Online: use services.gradle.org JSON API
    local resp, err = http.get({ url = API_URL })
    if err ~= nil then
        error("Failed to fetch Gradle versions: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("HTTP " .. resp.status_code)
    end

    local data = json.decode(resp.body)
    local stable = {}
    local rc_list = {}
    local seen = {}

    for _, item in ipairs(data) do
        if not item.snapshot and not item.nightly and not item.releaseNightly then
            local ver = item.version
            if ver and not seen[ver] then
                seen[ver] = true
                local is_rc = (item.rcFor and item.rcFor ~= "") or ver:find("%-rc%-") or ver:find("%-milestone%-")
                if is_rc then
                    table.insert(rc_list, { version = ver, note = (item.rcFor and item.rcFor ~= "") and ("RC for " .. item.rcFor) or "preview" })
                else
                    table.insert(stable, { version = ver, note = "" })
                end
            end
        end
    end

    -- Sort stable versions descending by semver components
    local function ver_cmp(a, b)
        local function parts(s)
            local t = {}
            for n in (s.version .. ".0.0"):gmatch("(%d+)") do
                table.insert(t, tonumber(n))
                if #t == 3 then break end
            end
            return t
        end
        local pa, pb = parts(a), parts(b)
        for i = 1, 3 do
            if (pa[i] or 0) ~= (pb[i] or 0) then
                return (pa[i] or 0) > (pb[i] or 0)
            end
        end
        return false
    end
    table.sort(stable, ver_cmp)
    table.sort(rc_list, ver_cmp)

    -- Stable versions first, then RC/previews
    for _, v in ipairs(stable) do table.insert(result, v) end
    for _, v in ipairs(rc_list) do table.insert(result, v) end

    return result
end
