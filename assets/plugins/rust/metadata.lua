PLUGIN = {
    name = "rust",
    version = "1.0.0",
    description = "Rust toolchain (via rustup)",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/rust/metadata.lua",
    homepage = "https://www.rust-lang.org",
    minRuntimeVersion = "0.3.0",

    mirrors = {
        {
            name = "default",
            description = "Official (static.rust-lang.org)",
            vars = { SDK_RUSTUP_MIRROR = "https://static.rust-lang.org" }
        },
        {
            name = "china",
            description = "rsproxy.cn (SJTU mirror)",
            vars = { SDK_RUSTUP_MIRROR = "https://rsproxy.cn" }
        },
        {
            name = "ustc",
            description = "USTC mirror",
            vars = { SDK_RUSTUP_MIRROR = "https://mirrors.ustc.edu.cn/rust-static" }
        },
        {
            name = "tuna",
            description = "Tsinghua TUNA mirror",
            vars = { SDK_RUSTUP_MIRROR = "https://mirrors.tuna.tsinghua.edu.cn/rustup" }
        },
        {
            name = "local",
            description = "Local file system mirror (mirror.local_dir/rust)",
            vars = { SDK_RUSTUP_MIRROR = "{local_dir}/rust" }
        },
        {
            name = "http-server",
            description = "Local HTTP mirror server (mirror.http_server/rust)",
            vars = {
                SDK_RUSTUP_MIRROR = "{http_server}/rust",
                SDK_FLAT_MIRROR   = "1"
            }
        },
    }
}
