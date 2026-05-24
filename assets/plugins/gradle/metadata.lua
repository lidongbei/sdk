PLUGIN = {
    name = "gradle",
    version = "1.0.0",
    description = "Gradle build tool",
    updateUrl = "https://raw.githubusercontent.com/lidongbei/sdk-plugins/main/gradle/metadata.lua",
    homepage = "https://gradle.org",
    minRuntimeVersion = "0.3.0",
    legacyFilenames = {".gradle-version"},

    mirrors = {
        {
            name = "default",
            description = "Official (services.gradle.org)",
            vars = {
                SDK_GRADLE_API    = "https://services.gradle.org/versions/all",
                SDK_GRADLE_MIRROR = "https://services.gradle.org/distributions"
            }
        },
        {
            name = "china",
            description = "Tencent Cloud Gradle mirror — full archive, ~90ms",
            vars = {
                SDK_GRADLE_API    = "https://services.gradle.org/versions/all",
                SDK_GRADLE_MIRROR = "https://mirrors.cloud.tencent.com/gradle"
            }
        },
        {
            name = "huawei",
            description = "Huawei Cloud Gradle mirror — full archive, ~170ms",
            vars = {
                SDK_GRADLE_API    = "https://services.gradle.org/versions/all",
                SDK_GRADLE_MIRROR = "https://mirrors.huaweicloud.com/gradle"
            }
        },
        {
            name = "local",
            description = "Local file system mirror (mirror.local_dir/gradle)",
            vars = {
                SDK_GRADLE_API    = "{local_dir}/gradle",
                SDK_GRADLE_MIRROR = "{local_dir}/gradle"
            }
        },
        {
            name = "http-server",
            description = "Local HTTP mirror server (mirror.http_server/gradle)",
            vars = {
                SDK_GRADLE_API    = "{http_server}/gradle",
                SDK_GRADLE_MIRROR = "{http_server}/gradle",
                SDK_FLAT_MIRROR   = "1"
            }
        },
    }
}
