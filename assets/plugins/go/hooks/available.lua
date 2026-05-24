local http = require("http")
local json = require("json")

local DL_URL = os.getenv("SDK_GO_MIRROR") or "https://go.dev"
-- SDK_GO_API: URL for version JSON API (always official or golang.google.cn)
local API_URL = os.getenv("SDK_GO_API") or "https://go.dev"

local function is_flat(path)
    -- Use flat file structure when: local filesystem path OR http-server profile
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_GO_FLAT") == "1"
end

local function is_local_path(url)
    -- Only treat as local when it's not an HTTP URL (filesystem path)
    return url:sub(1, 4) ~= "http"
end

function PLUGIN:Available(ctx)
    local result = {}

    if is_local_path(API_URL) then
        -- Local mirror: read versions.json (simple array of version strings)
        local resp, err = http.get({ url = API_URL .. "/versions.json" })
        if err ~= nil then
            error("Failed to read local Go versions: " .. tostring(err))
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

    -- Use API URL for version listing (always official go.dev or golang.google.cn)
    local resp, err = http.get({ url = API_URL .. "/dl/?mode=json&include=all" })
    if err ~= nil then
        error("Failed to fetch Go versions: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("HTTP " .. resp.status_code)
    end

    local data = json.decode(resp.body)

    for _, item in ipairs(data) do
        if item.stable then
            local version = item.version:gsub("^go", "")
            table.insert(result, { version = version })
        end
    end

    return result
end
