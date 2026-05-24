PLUGIN = {
    name = "maven",
    version = "1.0.0",
    description = "Apache Maven build tool",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/maven/metadata.lua",
    homepage = "https://maven.apache.org",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".mvn-version"},

    mirrors = {
        {
            name = "default",
            description = "Apache CDN (dlcdn.apache.org)",
            vars = { SDK_MAVEN_MIRROR = "https://dlcdn.apache.org/maven" }
        },
        {
            name = "archive",
            description = "Apache Archive (archive.apache.org) — full history incl. older releases",
            vars = { SDK_MAVEN_MIRROR = "https://archive.apache.org/dist/maven" }
        },
        {
            name = "china",
            description = "Huawei Cloud Apache mirror — full archive, ~200ms",
            vars = { SDK_MAVEN_MIRROR = "https://mirrors.huaweicloud.com/apache/maven" }
        },
        {
            name = "aliyun",
            description = "Aliyun Apache mirror — latest version only",
            vars = { SDK_MAVEN_MIRROR = "https://mirrors.aliyun.com/apache/maven" }
        },
        {
            name = "tencent",
            description = "Tencent Cloud Apache mirror — latest version only",
            vars = { SDK_MAVEN_MIRROR = "https://mirrors.cloud.tencent.com/apache/maven" }
        },
        {
            name = "local",
            description = "Local file system mirror (mirror.local_dir/maven)",
            vars = { SDK_MAVEN_MIRROR = "{local_dir}/maven" }
        },
        {
            name = "http-server",
            description = "Local HTTP mirror server (mirror.http_server/maven)",
            vars = {
                SDK_MAVEN_MIRROR = "{http_server}/maven",
                SDK_FLAT_MIRROR  = "1"
            }
        },
    }
}
