local http = require("http")
local json = require("json")

-- Fetch stable Rust releases from GitHub releases API
-- Override: export SDK_RUST_MIRROR=https://my-mirror
local GH_API = os.getenv("SDK_RUST_API") or "https://api.github.com"

function PLUGIN:Available(ctx)
    local resp, err = http.get({
        url = GH_API .. "/repos/rust-lang/rust/releases?per_page=20",
        headers = { ["Accept"] = "application/vnd.github+json" }
    })
    if err ~= nil then
        error("Failed to fetch Rust versions: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("HTTP " .. resp.status_code)
    end

    local data = json.decode(resp.body)
    local result = {}

    for _, rel in ipairs(data) do
        if not rel.prerelease and not rel.draft then
            local ver = rel.tag_name:gsub("^v", "")
            table.insert(result, { version = ver })
        end
    end

    -- Always include "stable", "beta", "nightly" as pseudo-versions
    table.insert(result, 1, { version = "nightly", note = "latest nightly" })
    table.insert(result, 1, { version = "beta",    note = "latest beta" })
    table.insert(result, 1, { version = "stable",  note = "latest stable" })

    return result
end
