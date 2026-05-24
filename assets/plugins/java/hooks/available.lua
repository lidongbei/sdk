local http = require("http")
local json = require("json")

-- Source: adoptium (default) | oracle | zulu
local SOURCE  = os.getenv("SDK_JAVA_SOURCE") or "adoptium"
local API_URL = os.getenv("SDK_JAVA_API")    or "https://api.adoptium.net/v3"

-- Oracle JDK: no public JSON API; maintain static list of current LTS/GA releases.
-- Oracle only hosts "latest" downloads for actively supported versions.
local ORACLE_VERSIONS = {
    { version = "25", note = "LTS" },
    { version = "21", note = "LTS" },
}

-- Standard Java LTS versions (used for Zulu LTS annotation)
local JAVA_LTS = { ["8"]=true, ["11"]=true, ["17"]=true, ["21"]=true, ["25"]=true }

function PLUGIN:Available(ctx)
    -- Oracle JDK source: return static curated version list
    if SOURCE == "oracle" then
        return ORACLE_VERSIONS
    end

    -- Azul Zulu: query metadata API for all available major versions
    if SOURCE == "zulu" then
        local zulu_api = API_URL or "https://api.azul.com/metadata/v1"
        local url = zulu_api .. "/zulu/packages/?java_package_type=jdk&archive_type=tar.gz&latest=true&page_size=200"
        local resp, err = http.get({ url = url })
        if err ~= nil then
            error("Failed to fetch Zulu releases: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("Zulu API HTTP " .. resp.status_code)
        end
        local data = json.decode(resp.body)
        local seen = {}
        local result = {}
        for _, pkg in ipairs(data) do
            -- Skip CRaC and JavaFX-bundled variants
            local name = pkg.name or ""
            if name:find("%-crac%-") or name:find("%-fx%-") then
                goto continue
            end
            local major = tostring(pkg.java_version[1])
            if not seen[major] then
                seen[major] = true
                -- Build full version string: e.g. "17.0.19"
                local jv = pkg.java_version
                local full_ver
                if jv[3] and jv[3] > 0 then
                    full_ver = string.format("%d.%d.%d", jv[1], jv[2], jv[3])
                elseif jv[2] and jv[2] > 0 then
                    full_ver = string.format("%d.%d", jv[1], jv[2])
                else
                    full_ver = tostring(jv[1])
                end
                table.insert(result, { version = full_ver, note = JAVA_LTS[major] and "LTS" or "" })
            end
            ::continue::
        end
        table.sort(result, function(a, b)
            local ma = tonumber(a.version:match("^(%d+)")) or 0
            local mb = tonumber(b.version:match("^(%d+)")) or 0
            return ma > mb
        end)
        return result
    end

    -- Adoptium / Temurin source (default)
    local result = {}
    local all_versions = {}
    local lts_set = {}

    local MIRROR_URL = os.getenv("SDK_JAVA_MIRROR") or ""

    -- Hierarchical HTTP mirror (e.g. Tsinghua): discover versions from mirror directory
    -- URL structure: {mirror}/{major}/jdk/{arch}/{os}/{filename}
    local function is_flat(path)
        return path:sub(1, 4) ~= "http" or os.getenv("SDK_JAVA_FLAT") == "1"
    end

    if MIRROR_URL ~= "" and not is_flat(MIRROR_URL) then
        -- Step 1: list root to find available major versions
        local resp, err = http.get({ url = MIRROR_URL .. "/" })
        if err ~= nil or resp.status_code ~= 200 then
            error("Failed to fetch mirror directory: " .. tostring(err or resp.status_code))
        end
        -- Extract major version directories like "17/", "21/"
        local majors = {}
        for m in resp.body:gmatch('"(%d+)/"') do
            table.insert(majors, m)
        end
        -- Sort descending by numeric major version
        table.sort(majors, function(a, b) return tonumber(a) > tonumber(b) end)

        -- Standard Java LTS versions
        for _, v in ipairs({8, 11, 17, 21, 25}) do lts_set[tostring(v)] = true end

        for _, major in ipairs(majors) do
            -- Query the arch/os directory to get filename with full version
            local dir_url = string.format("%s/%s/jdk/x64/linux/", MIRROR_URL, major)
            local dr, de = http.get({ url = dir_url })
            local full_ver = major  -- fallback to major only
            if de == nil and dr.status_code == 200 then
                -- Extract version from filename: OpenJDK17U-jdk_x64_linux_hotspot_17.0.19_10.tar.gz
                -- Also handles 4-part: hotspot_18.0.2.1_1  → normalize to 3-part
                local patch = dr.body:match("hotspot_(%d+%.%d+%.%d+)%.%d+_")
                             or dr.body:match("hotspot_(%d+%.%d+%.%d+)_")
                if patch then full_ver = patch end
            end
            table.insert(result, {
                version = full_ver,
                note = lts_set[major] and "LTS" or ""
            })
        end
        return result
    end

    -- Use Adoptium API for version discovery
    local resp, err = http.get({ url = API_URL .. "/info/available_releases" })
    if err ~= nil then
        error("Failed to fetch Java releases: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("HTTP " .. resp.status_code)
    end

    local data = json.decode(resp.body)
    for _, v in ipairs(data.available_lts_releases or {}) do
        lts_set[tostring(v)] = true
    end

    -- available_releases is array of feature versions (integers)
    for _, ver in ipairs(data.available_releases or {}) do
        table.insert(all_versions, tostring(ver))
    end

    -- Sort descending
    table.sort(all_versions, function(a, b)
        return tonumber(a) > tonumber(b)
    end)

    for _, feature_ver in ipairs(all_versions) do
        -- Query the latest release for this major version to get full patch version
        local patch_url = string.format(
            "%s/assets/latest/%s/hotspot?architecture=x64&image_type=jdk&jvm_impl=hotspot&os=linux&vendor=eclipse",
            API_URL, feature_ver
        )
        local pr, pe = http.get({ url = patch_url })
        local full_ver = feature_ver
        if pe == nil and pr.status_code == 200 then
            local pd = json.decode(pr.body)
            if pd and #pd > 0 and pd[1].version then
                local v = pd[1].version
                -- semver may be "17.0.19+7" or "18.0.2.1+1"
                local sv = v.semver or ""
                full_ver = sv:match("^(%d+%.%d+%.%d+)") or feature_ver
            end
        end
        table.insert(result, {
            version = full_ver,
            note = lts_set[feature_ver] and "LTS" or ""
        })
    end

    return result
end
