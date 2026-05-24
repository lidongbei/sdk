local MIRROR_BASE = os.getenv("SDK_MAVEN_MIRROR") or "https://dlcdn.apache.org/maven"

local function is_flat(path)
    return path:sub(1, 4) ~= "http" or os.getenv("SDK_MAVEN_FLAT") == "1"
end

-- Extract major version number from version string (e.g. "3.9.10" → "3", "4.0.0" → "4")
local function major_ver(version)
    return version:match("^(%d+)%.") or "3"
end

function PLUGIN:PreInstall(ctx)
    local version = ctx.version
    local major = major_ver(version)

    local filename
    if OS_TYPE == "windows" then
        filename = string.format("apache-maven-%s-bin.zip", version)
    else
        filename = string.format("apache-maven-%s-bin.tar.gz", version)
    end

    if is_flat(MIRROR_BASE) then
        -- Local mirror: flat structure
        return {
            version = version,
            url = MIRROR_BASE .. "/" .. filename,
        }
    end

    -- e.g. https://dlcdn.apache.org/maven/maven-3/3.9.16/binaries/apache-maven-3.9.16-bin.tar.gz
    --      https://dlcdn.apache.org/maven/maven-4/4.0.0/binaries/apache-maven-4.0.0-bin.tar.gz
    local url = string.format("%s/maven-%s/%s/binaries/%s",
        MIRROR_BASE, major, version, filename)

    return {
        version = version,
        url = url,
    }
end
