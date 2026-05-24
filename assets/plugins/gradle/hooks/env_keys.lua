function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    return {
        { key = "GRADLE_HOME", value = main_path },
        { key = "PATH",        value = main_path .. "/bin" },
    }
end
