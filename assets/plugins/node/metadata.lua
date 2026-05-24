PLUGIN = {
    name = "node",
    version = "1.0.0",
    description = "Node.js JavaScript runtime",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/node/metadata.lua",
    homepage = "https://nodejs.org",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".nvmrc", ".node-version"},

    -- Mirror profiles: switch with `sdk mirror use <profile> [node]`
    -- Use `sdk config set mirror.local_dir /your/path` to configure the local mirror directory.
    -- Defaults to ~/.sdk/downloads/ when not set.
    mirrors = {
        {
            name = "default",
            description = "Official (nodejs.org)",
            vars = { SDK_NODE_MIRROR = "https://nodejs.org/dist" }
        },
        {
            name = "china",
            description = "npmmirror CDN (China)",
            vars = { SDK_NODE_MIRROR = "https://registry.npmmirror.com/-/binary/node" }
        },
        {
            name = "tencent",
            description = "Tencent Cloud mirror",
            vars = { SDK_NODE_MIRROR = "https://mirrors.cloud.tencent.com/nodejs-release" }
        },
        {
            name = "huawei",
            description = "Huawei Cloud mirror",
            vars = { SDK_NODE_MIRROR = "https://mirrors.huaweicloud.com/nodejs" }
        },
        {
            name = "local",
            description = "Local file system mirror (mirror.local_dir/node)",
            vars = { SDK_NODE_MIRROR = "{local_dir}/node" }
        },
        {
            name = "http-server",
            description = "Local HTTP mirror server (mirror.http_server/node)",
            vars = {
                SDK_NODE_MIRROR = "{http_server}/node",
                SDK_NODE_FLAT = "1"
            }
        },
    }
}
