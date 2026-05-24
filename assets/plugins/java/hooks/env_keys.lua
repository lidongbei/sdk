function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    local os_type = OS_TYPE

    local java_home = main_path
    -- On macOS, Temurin unpacks with Contents/Home subfolder
    if os_type == "darwin" then
        local sub = main_path .. "/Contents/Home"
        -- Check if Contents/Home exists (it often does for .tar.gz on macOS)
        java_home = sub
    end

    return {
        { key = "JAVA_HOME", value = java_home },
        { key = "PATH",      value = java_home .. "/bin" },
    }
end
