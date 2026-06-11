function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    local os_type = OS_TYPE

    if os_type == "windows" then
        return {
            { key = "PATH", value = main_path },
        }
    else
        return {
            { key = "PATH", value = main_path .. "/bin" },
        }
    end
end
