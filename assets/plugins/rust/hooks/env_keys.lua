function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    return {
        { key = "RUSTUP_HOME",   value = main_path .. "/rustup" },
        { key = "CARGO_HOME",    value = main_path .. "/cargo" },
        { key = "PATH",          value = main_path .. "/cargo/bin" },
    }
end
