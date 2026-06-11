PLUGIN = {
    name = "python",
    version = "1.0.0",
    description = "Python 自动选择：有 uv 则用 uv，否则直接下载 python-build-standalone",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/python/metadata.lua",
    homepage = "https://www.python.org",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".python-version"},

    -- 合并 python 和 py-uv 的 mirror 配置
    -- uv 路径：通过 UV_PYTHON_INSTALL_MIRROR 控制下载源（uv 内置支持）
    -- 直接路径：通过 SDK_PYTHON_STANDALONE_MIRROR / SDK_PYTHON_STANDALONE_API 控制
    mirrors = {
        {
            name        = "default",
            description = "自动选择：有 uv 则用 uv（GitHub releases），无 uv 则直接下载 python-build-standalone",
            vars = {
                SDK_UV_BIN                   = "uv",
                SDK_PYTHON_MIRROR            = "https://www.python.org",
                SDK_PYTHON_STANDALONE_MIRROR = "https://github.com/astral-sh/python-build-standalone/releases/download",
                SDK_PYTHON_STANDALONE_API    = "",
            }
        },
        {
            name        = "china",
            description = "npmmirror CDN（国内加速）— uv 和直接下载均通过 registry.npmmirror.com",
            vars = {
                SDK_UV_BIN                   = "uv",
                UV_PYTHON_INSTALL_MIRROR     = "https://registry.npmmirror.com/-/binary/python-build-standalone",
                SDK_PYTHON_MIRROR            = "https://mirrors.huaweicloud.com/python",
                SDK_PYTHON_STANDALONE_MIRROR = "https://registry.npmmirror.com/-/binary/python-build-standalone",
                SDK_PYTHON_STANDALONE_API    = "https://registry.npmmirror.com/-/binary/python-build-standalone",
            }
        },
        {
            name        = "huawei",
            description = "华为云镜像（通过 npmmirror 分发 python-build-standalone）",
            vars = {
                SDK_UV_BIN                   = "uv",
                UV_PYTHON_INSTALL_MIRROR     = "https://registry.npmmirror.com/-/binary/python-build-standalone",
                SDK_PYTHON_MIRROR            = "https://mirrors.huaweicloud.com/python",
                SDK_PYTHON_STANDALONE_MIRROR = "https://registry.npmmirror.com/-/binary/python-build-standalone",
                SDK_PYTHON_STANDALONE_API    = "https://registry.npmmirror.com/-/binary/python-build-standalone",
            }
        },
        {
            name        = "local",
            description = "本地文件系统镜像（仅 python-build-standalone 路径，不使用 uv）",
            vars = {
                SDK_PYTHON_MIRROR            = "{local_dir}/python",
                SDK_PYTHON_STANDALONE_MIRROR = "{local_dir}/python-standalone",
                SDK_PYTHON_STANDALONE_API    = "",
            }
        },
        {
            name        = "http-server",
            description = "本地 HTTP 镜像服务器（仅 python-build-standalone 路径，不使用 uv）",
            vars = {
                SDK_PYTHON_MIRROR            = "{http_server}/python",
                SDK_PYTHON_STANDALONE_MIRROR = "{http_server}/python-standalone",
                SDK_PYTHON_STANDALONE_API    = "",
                SDK_FLAT_MIRROR              = "1",
            }
        },
    }
}
