function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    return {
        { key = "MAVEN_HOME", value = main_path },
        { key = "M2_HOME",    value = main_path },
        { key = "PATH",       value = main_path .. "/bin" },
    }
end
