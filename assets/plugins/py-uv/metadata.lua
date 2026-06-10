PLUGIN = {
    name = "py-uv",
    version = "1.0.0",
    description = "Python uv 管理",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/py-uv/metadata.lua",
    homepage = "https://www.python.org",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".py-uv-version"},

    -- uv 通过 UV_PYTHON_INSTALL_MIRROR 控制 python-build-standalone 的下载源
    -- 默认来源：https://github.com/astral-sh/python-build-standalone/releases/download
    mirrors = {
        {
            name        = "default",
            description = "默认 uv 配置 — 通过 GitHub releases 下载 python-build-standalone",
            vars = {
                SDK_UV_BIN = "uv",
            }
        },
        {
            name        = "china",
            description = "npmmirror CDN（国内加速）— 通过 registry.npmmirror.com 下载",
            vars = {
                SDK_UV_BIN               = "uv",
                -- uv 将使用此 URL 作为 python-build-standalone 的下载基础路径
                -- 下载格式：{UV_PYTHON_INSTALL_MIRROR}/{tag}/{filename}
                UV_PYTHON_INSTALL_MIRROR = "https://registry.npmmirror.com/-/binary/python-build-standalone",
            }
        },
        {
            name        = "huawei",
            description = "华为云镜像（通过 npmmirror 分发 python-build-standalone）",
            vars = {
                SDK_UV_BIN               = "uv",
                UV_PYTHON_INSTALL_MIRROR = "https://registry.npmmirror.com/-/binary/python-build-standalone",
            }
        },
    }
}
