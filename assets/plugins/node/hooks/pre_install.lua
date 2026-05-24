local BASE_URL = os.getenv("SDK_NODE_MIRROR") or "https://nodejs.org/dist"

local function is_flat(path)
    -- Use flat file structure when: local filesystem path OR http-server profile
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_NODE_FLAT") == "1"
end

function PLUGIN:PreInstall(ctx)
    local version = ctx.version
    local os_type = OS_TYPE   -- "darwin", "linux", "windows"
    local arch = ARCH_TYPE    -- "amd64", "arm64", "386"

    -- Map OS and arch to Node.js naming convention
    local os_name
    if os_type == "darwin" then
        os_name = "darwin"
    elseif os_type == "linux" then
        os_name = "linux"
    elseif os_type == "windows" then
        os_name = "win"
    else
        error("Unsupported OS: " .. tostring(os_type))
    end

    local arch_name
    if arch == "amd64" then
        arch_name = "x64"
    elseif arch == "arm64" then
        arch_name = "arm64"
    elseif arch == "386" then
        arch_name = "x86"
    else
        error("Unsupported arch: " .. tostring(arch))
    end

    local ext
    if os_type == "windows" then
        ext = "zip"
    else
        ext = "tar.xz"
    end

    local filename = string.format("node-v%s-%s-%s.%s", version, os_name, arch_name, ext)

    local url
    if is_flat(BASE_URL) then
        -- Local mirror: flat structure — files stored directly as <base>/<filename>
        url = BASE_URL .. "/" .. filename
    else
        url = string.format("%s/v%s/%s", BASE_URL, version, filename)
    end

    return {
        version = version,
        url = url,
    }
end
