local http = require("http")
local json = require("json")

local BASE_URL = os.getenv("SDK_NODE_MIRROR") or "https://nodejs.org/dist"

local function is_flat(path)
    -- Use flat file structure when: local filesystem path OR http-server profile
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_NODE_FLAT") == "1"
end

function PLUGIN:Available(ctx)
    local result = {}

    if is_flat(BASE_URL) then
        -- Local mirror: read versions.json (simple array of version strings)
        -- e.g. ["24.16.0", "22.16.0", "20.19.2"]
        local resp, err = http.get({ url = BASE_URL .. "/versions.json" })
        if err ~= nil then
            error("Failed to read local Node.js versions: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("versions.json not found at " .. BASE_URL)
        end
        local data = json.decode(resp.body)
        for _, ver in ipairs(data) do
            table.insert(result, { version = tostring(ver) })
        end
        return result
    end

    local resp, err = http.get({ url = BASE_URL .. "/index.json" })
    if err ~= nil then
        error("Failed to fetch Node.js versions: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("HTTP " .. resp.status_code .. " from " .. BASE_URL)
    end

    local data = json.decode(resp.body)

    for _, item in ipairs(data) do
        local version = item.version:gsub("^v", "")
        local note = ""
        if item.lts and item.lts ~= false then
            note = "LTS (" .. tostring(item.lts) .. ")"
        end
        table.insert(result, { version = version, note = note })
    end

    return result
end
