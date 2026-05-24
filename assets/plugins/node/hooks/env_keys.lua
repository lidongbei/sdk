function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    return {
        { key = "PATH", value = main_path .. "/bin" },
    }
end
