local DL_URL = os.getenv("SDK_GO_MIRROR") or "https://go.dev"
-- Official mirror in China: https://gomirrors.org or https://dl.google.com/go
-- Set: export SDK_GO_MIRROR=https://dl.google.com/go  (if go.dev is blocked)

local function is_flat(path)
    -- Use flat file structure when: local filesystem path OR http-server profile
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_GO_FLAT") == "1"
end

function PLUGIN:PreInstall(ctx)
    local version = ctx.version
    local os_type = OS_TYPE
    local arch = ARCH_TYPE

    local os_name
    if os_type == "darwin" then
        os_name = "darwin"
    elseif os_type == "linux" then
        os_name = "linux"
    elseif os_type == "windows" then
        os_name = "windows"
    else
        error("Unsupported OS: " .. tostring(os_type))
    end

    local arch_name
    if arch == "amd64" then
        arch_name = "amd64"
    elseif arch == "arm64" then
        arch_name = "arm64"
    else
        error("Unsupported arch: " .. tostring(arch))
    end

    local ext
    if os_type == "windows" then
        ext = "zip"
    else
        ext = "tar.gz"
    end

    local filename = string.format("go%s.%s-%s.%s", version, os_name, arch_name, ext)

    local url
    if is_flat(DL_URL) then
        -- Local mirror: flat structure — files stored directly as <base>/<filename>
        url = DL_URL .. "/" .. filename
    else
        url = string.format("%s/dl/%s", DL_URL, filename)
    end

    return {
        version = version,
        url = url,
    }
end
