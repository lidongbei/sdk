function PLUGIN:EnvKeys(ctx)
    local main_path = ctx.main.path
    local os_type   = OS_TYPE

    -- SDK 将 python-build-standalone 的 install_only 压缩包解压后
    -- 会自动剥离顶层 python/ 目录（strip_toplevel_dir），结果：
    --   Unix:    main_path/bin/python3.X
    --   Windows: main_path/python.exe
    if os_type == "windows" then
        return { { key = "PATH", value = main_path } }
    else
        return { { key = "PATH", value = main_path .. "/bin" } }
    end
end
