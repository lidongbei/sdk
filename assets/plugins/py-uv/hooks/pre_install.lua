local json    = require("json")
local BASE_BIN = os.getenv("SDK_UV_BIN") or "uv"

function PLUGIN:PreInstall(ctx)
    local version = ctx.version

    -- 从 uv 获取该版本的下载 URL，由 SDK 负责下载和解压
    -- uv python list 对"可下载但未安装"的版本返回 url 且 path 为空
    local cmd = string.format('"%s" python list --all-versions --output-format json', BASE_BIN)
    local handle = io.popen(cmd, "r")
    if not handle then
        error("无法执行 uv python list，请确认 uv 已安装并在 PATH 中")
    end
    local output = handle:read("*a")
    handle:close()

    local ok, data = pcall(json.decode, output)
    if not ok or type(data) ~= "table" then
        error("解析 uv python list 输出失败")
    end

    -- 查找匹配版本且有下载 URL 的条目
    -- 注意：JSON null 在 mlua 中为 userdata，不是 nil 或 ""，需用 type() 判断
    -- path 为 null/空字符串 = 未安装（仅可下载）；variant = "default" = install_only_stripped 标准版本
    for _, item in ipairs(data) do
        if item.implementation == "cpython"
            and item.version == version
            and type(item.url) == "string" and item.url ~= ""
            and (type(item.path) ~= "string" or item.path == "")
            and (item.variant == "default" or item.variant == nil or type(item.variant) ~= "string")
        then
            return { version = version, url = item.url }
        end
    end

    error(string.format(
        "未找到 Python %s 的下载 URL。\n" ..
        "可能该版本已被其他工具安装并被 uv 发现（uv 不再提供其下载地址）。\n" ..
        "建议改用: sdk install python@%s",
        version, version
    ))
end
