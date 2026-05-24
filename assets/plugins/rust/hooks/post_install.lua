function PLUGIN:PostInstall(ctx)
    local root_path = ctx.rootPath
    local os_type = OS_TYPE
    local version = ctx.main and ctx.main.version or "stable"
    local rustup_mirror = os.getenv("SDK_RUSTUP_MIRROR") or ""

    -- Build environment for rustup
    local env = ""
    if rustup_mirror ~= "" then
        env = string.format(
            "RUSTUP_DIST_SERVER=%s RUSTUP_UPDATE_ROOT=%s/rustup ",
            rustup_mirror, rustup_mirror
        )
    end

    local rustup_init
    if os_type == "windows" then
        rustup_init = root_path .. "/rustup-init.exe"
        -- Make it executable and run silently
        os.execute(string.format(
            '"%s" -y --default-toolchain %s --no-modify-path',
            rustup_init, version
        ))
    else
        rustup_init = root_path .. "/rustup-init"
        os.execute("chmod +x " .. rustup_init)
        os.execute(string.format(
            '%ssh RUSTUP_HOME=%s/rustup CARGO_HOME=%s/cargo %s -y --default-toolchain %s --no-modify-path',
            env, root_path, root_path, rustup_init, version
        ))
    end
end
