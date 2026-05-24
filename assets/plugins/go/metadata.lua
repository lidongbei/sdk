PLUGIN = {
    name = "go",
    version = "1.0.0",
    description = "Go programming language",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/go/metadata.lua",
    homepage = "https://go.dev",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".go-version"},

    mirrors = {
        {
            name = "default",
            description = "Official (go.dev)",
            vars = {
                SDK_GO_MIRROR = "https://go.dev",
                SDK_GO_API    = "https://go.dev"
            }
        },
        {
            name = "china",
            description = "golang.google.cn — official China mirror, same CDN as go.dev",
            vars = {
                SDK_GO_MIRROR = "https://golang.google.cn",
                SDK_GO_API    = "https://golang.google.cn"
            }
        },
        {
            name = "aliyun",
            description = "Aliyun mirror — flat structure, may not have all versions",
            vars = {
                SDK_GO_MIRROR = "https://mirrors.aliyun.com/golang",
                SDK_GO_API    = "https://golang.google.cn",
                SDK_GO_FLAT   = "1"
            }
        },
        {
            name = "local",
            description = "Local file system mirror (mirror.local_dir/go)",
            vars = {
                SDK_GO_MIRROR = "{local_dir}/go",
                SDK_GO_API    = "{local_dir}/go"
            }
        },
        {
            name = "http-server",
            description = "Local HTTP mirror server (mirror.http_server/go)",
            vars = {
                SDK_GO_MIRROR = "{http_server}/go",
                SDK_GO_API    = "{http_server}/go",
                SDK_GO_FLAT   = "1"
            }
        },
    }
}
