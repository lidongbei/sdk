local DL_URL = os.getenv("SDK_GRADLE_MIRROR") or "https://services.gradle.org/distributions"

function PLUGIN:PreInstall(ctx)
    local version = ctx.version
    -- e.g. https://services.gradle.org/distributions/gradle-8.5-bin.zip
    local filename = string.format("gradle-%s-bin.zip", version)
    local url = string.format("%s/%s", DL_URL, filename)

    return {
        version = version,
        url = url,
    }
end
