local http = require("http")
local json = require("json")

local SOURCE     = os.getenv("SDK_JAVA_SOURCE") or "adoptium"
local API_URL    = os.getenv("SDK_JAVA_API")    or "https://api.adoptium.net/v3"
local MIRROR_URL = os.getenv("SDK_JAVA_MIRROR") or ""
local ZULU_API   = (SOURCE == "zulu" and API_URL ~= "") and API_URL or "https://api.azul.com/metadata/v1"
local ZULU_CDN   = MIRROR_URL ~= "" and MIRROR_URL or "https://cdn.azul.com/zulu/bin"

local function is_flat(path)
    -- Use flat file structure when: local filesystem path OR http-server profile
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_JAVA_FLAT") == "1"
end

function PLUGIN:PreInstall(ctx)
    local feature_version = ctx.version
    local os_type = OS_TYPE
    local arch    = ARCH_TYPE

    local arch_name
    if arch == "amd64" then
        arch_name = "x64"
    elseif arch == "arm64" then
        arch_name = "aarch64"
    else
        error("Unsupported arch: " .. tostring(arch))
    end

    -- Oracle JDK: construct direct download URL from download.oracle.com
    -- Only versions currently hosted under /latest/ (21, 25).
    if SOURCE == "oracle" then
        local os_name
        if os_type == "darwin" then
            os_name = "macos"
        elseif os_type == "linux" then
            os_name = "linux"
        elseif os_type == "windows" then
            os_name = "windows"
        else
            error("Unsupported OS: " .. tostring(os_type))
        end
        local ext = (os_type == "windows") and "zip" or "tar.gz"
        local filename = string.format("jdk-%s_%s-%s_bin.%s",
            feature_version, os_name, arch_name, ext)
        local url = string.format("https://download.oracle.com/java/%s/latest/%s",
            feature_version, filename)
        return { version = feature_version, url = url }
    end

    -- Azul Zulu: query metadata API for exact download URL
    if SOURCE == "zulu" then
        local os_name
        if os_type == "darwin" then
            os_name = "macos"
        elseif os_type == "linux" then
            os_name = "linux"
        elseif os_type == "windows" then
            os_name = "windows"
        else
            error("Unsupported OS: " .. tostring(os_type))
        end
        local archive_type = (os_type == "windows") and "zip" or "tar.gz"
        local url = string.format(
            "%s/zulu/packages/?java_version=%s&os=%s&arch=%s&java_package_type=jdk&archive_type=%s&latest=true",
            ZULU_API, feature_version, os_name, arch_name, archive_type
        )
        local resp, err = http.get({ url = url })
        if err ~= nil then
            error("Failed to query Zulu API: " .. tostring(err))
        end
        if resp.status_code ~= 200 then
            error("Zulu API HTTP " .. resp.status_code)
        end
        local data = json.decode(resp.body)
        -- Skip JavaFX-bundled and CRaC variant packages; prefer plain JDK
        local chosen
        for _, pkg in ipairs(data) do
            local name = pkg.name or ""
            if not name:find("%-fx%-") and not name:find("%-crac%-") then
                chosen = pkg
                break
            end
        end
        if not chosen then
            -- Fallback: accept any non-fx package
            for _, pkg in ipairs(data) do
                if not (pkg.name or ""):find("%-fx%-") then
                    chosen = pkg
                    break
                end
            end
        end
        if not chosen then chosen = data[1] end
        if not chosen then
            error("No Zulu JDK found for Java " .. feature_version)
        end
        -- If a mirror is set, rewrite the CDN base URL in download_url
        local dl_url = chosen.download_url
        local fallback_url = ""
        if MIRROR_URL ~= "" then
            -- cdn.azul.com/zulu/bin/filename -> ZULU_CDN/filename
            local filename = dl_url:match("/([^/]+)$")
            fallback_url = dl_url   -- keep official CDN URL as fallback
            dl_url = ZULU_CDN .. "/" .. filename
        end
        return { version = feature_version, url = dl_url, fallback_url = fallback_url }
    end

    -- Adoptium / Temurin (default)
    local os_name
    if os_type == "darwin" then
        os_name = "mac"
    elseif os_type == "linux" then
        os_name = "linux"
    elseif os_type == "windows" then
        os_name = "windows"
    else
        error("Unsupported OS: " .. tostring(os_type))
    end

    -- flat mirror (local / http-server): download from pre-built flat dir
    -- Adoptium API requires major version number only (e.g. "17" not "17.0.19")
    local major_version = feature_version:match("^(%d+)")
    if MIRROR_URL ~= "" and is_flat(MIRROR_URL) then
        local ext = (os_type == "windows") and "zip" or "tar.gz"
        local filename = string.format(
            "OpenJDK%sU-jdk_%s_%s_hotspot_latest.%s",
            major_version, arch_name, os_name, ext
        )
        return {
            version = feature_version,
            url     = MIRROR_URL .. "/" .. filename,
        }
    end

    -- Use Adoptium API to find the exact download URL / filename
    local api_url = string.format(
        "%s/assets/latest/%s/hotspot?architecture=%s&image_type=jdk&jvm_impl=hotspot&os=%s&vendor=eclipse",
        API_URL, major_version, arch_name, os_name
    )
    local resp, err = http.get({ url = api_url })
    if err ~= nil then
        error("Failed to query Adoptium API: " .. tostring(err))
    end
    if resp.status_code ~= 200 then
        error("Adoptium API returned HTTP " .. resp.status_code)
    end

    local data = json.decode(resp.body)
    if not data or #data == 0 then
        error("No release found for Java " .. feature_version)
    end

    local release   = data[1]
    local pkg       = release.binary.package
    local filename  = pkg.name   -- e.g. OpenJDK21U-jdk_x64_linux_hotspot_21.0.11_10.tar.gz
    -- Extract full semver from release name (e.g. "jdk-21.0.11+10" → "21.0.11")
    local semver = release.version and release.version.semver or feature_version
    local actual_version = semver:match("^(%d+%.%d+%.%d+)") or feature_version

    local url
    if MIRROR_URL ~= "" then
        -- Hierarchical HTTP mirror (e.g. Tsinghua): {mirror}/{major_version}/jdk/{arch}/{os}/{filename}
        url = string.format("%s/%s/jdk/%s/%s/%s",
            MIRROR_URL, major_version, arch_name, os_name, filename)
    else
        url = pkg.link
    end

    return {
        version = actual_version,
        url     = url,
    }
end
