local http = require("http")
local json = require("json")

local BASE_BIN = os.getenv("SDK_UV_BIN") or "uv"


function PLUGIN:Available(ctx)
    -- 列出所有可下载的 CPython 版本（含所有 patch 版本）
    local cmd = string.format('"%s" python list --all-versions --output-format json', BASE_BIN)
    local handle = io.popen(cmd, "r")
    if not handle then
        return {}
    end

    local output = handle:read("*a")
    local ok, _, code = handle:close()
    if not ok or code ~= 0 or output == "" then
        return {}
    end

    -- 解析 JSON
    local success, data = pcall(json.decode, output)
    if not success or type(data) ~= "table" then
        return {}
    end

    -- 提取版本，只保留 CPython，去重
    -- 注意：JSON null 在 mlua 中为 userdata，需用 type() 判断
    local seen   = {}
    local result = {}
    for _, item in ipairs(data) do
        if type(item.implementation) == "string" and item.implementation ~= "cpython" then goto continue end
        if type(item) == "table" and type(item.version) == "string" and item.version ~= "" then
            local ver = item.version
            if not seen[ver] then
                seen[ver] = true
                table.insert(result, { version = ver })
            end
        end
        ::continue::
    end
    return result
end
