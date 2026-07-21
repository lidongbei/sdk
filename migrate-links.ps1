# migrate-links.ps1
#
# 目录软链接迁移工具
#
# 用法：
#   迁移单个目录（在原机器上）—— 参数也可不指定，进入交互提示逐项输入：
#     .\migrate-links.ps1 -Source D:\Works\github\sdk
#     .\migrate-links.ps1 -Source D:\Works\github\sdk -KeepLevels 2 -LinkType SymbolicLink
#     .\migrate-links.ps1                       # 交互模式：提示输入 Source / KeepLevels / LinkType
#
#   在新机器上重建所有 link：
#     .\migrate-links.ps1 -Mount
#     .\migrate-links.ps1 -Mount -OldSourceRoot D:\Works -NewSourceRoot C:\Users\me\Works
#     .\migrate-links.ps1 -Mount -OldTargetRoot E:\migrated -NewTargetRoot F:\migrated
#
#   预览（不执行任何修改）：
#     .\migrate-links.ps1 -Source D:\Works\github\sdk -WhatIf
#     .\migrate-links.ps1 -Mount -WhatIf
#
#   强制迁移危险路径（用户配置文件根目录等；默认拒绝）：
#     .\migrate-links.ps1 -Source C:\Users\Administrator -Force
#
# 默认目标根目录 = 脚本所在文件夹（manifest 与迁移数据存于此，整个文件夹可整体搬到新机器）
# manifest 文件位置：<TargetRoot>\_symlink-migration.json
#
#   自定义目标路径（跳过 KeepLevels 拼接，任意整段映射）：
#     .\migrate-links.ps1 -Source "C:\Program Files\JetBrains" -TargetPath "D:\CLinks\JetBrains\Apps"
#
#   源不存在时直接创建空目录并建 link（跳过数据复制）：
#     .\migrate-links.ps1 -Source D:\new\dir -CreateEmptyLink
#     （交互模式下输入"2"也能达到同样效果）
#
#   在新机器上按 manifest 重建所有 link（目标和源缺失时自动创建）：
#     .\migrate-links.ps1 -Mount -CreateMissing
#     .\migrate-links.ps1 -Mount -OldSourceRoot D:\Works -NewSourceRoot C:\Users\me\Works -CreateMissing
#
#   路径参数支持环境变量（展开为绝对路径后再使用，manifest 中保存展开后的值）：
#     .\migrate-links.ps1 -Source "%USERPROFILE%\.config" -TargetRoot "$env:APPDATA\links"
#     .\migrate-links.ps1 -Mount -OldSourceRoot "%USERPROFILE%" -NewSourceRoot "C:\Users\me" -CreateMissing

param(
    # 目标父级目录（默认为脚本所在文件夹；可通过 -TargetRoot 覆盖）
    [string]$TargetRoot,

    [Parameter(Position = 0)]
    [string]$Source,

    # 保留源末尾层级数（从路径末尾向前取 N 段，不含盘符）
    [ValidateRange(1, 32)]
    [int]$KeepLevels = 1,

    # 自定义目标路径（与 KeepLevels 互斥；指定时直接以本路径为目标，跳过 KeepLevels 自动拼接）
    [string]$TargetPath,

    # 链接类型：Junction（无需特权，仅本地）/ SymbolicLink（需管理员或开发者模式）
    [ValidateSet("Junction", "SymbolicLink")]
    [string]$LinkType = "Junction",

    # 挂载模式：在新机器上读取 manifest 并重建所有 link
    [switch]$Mount,

    # 挂载时的路径前缀替换（任一为空则对应路径原样使用）
    [string]$OldSourceRoot,
    [string]$NewSourceRoot,
    [string]$OldTargetRoot,
    [string]$NewTargetRoot,

    # 强制迁移危险路径（如用户配置文件根目录；默认拒绝）
    [switch]$Force,

    # 源目录不存在时直接创建空目录并建立 link（跳过数据复制）
    [switch]$CreateEmptyLink,

    # 挂载模式：目标和源（含父级）不存在时也自动创建（用于全新机器恢复）
    [switch]$CreateMissing,

    # 预览模式：只打印将执行的操作，不实际修改
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"
# 默认目标根目录 = 脚本所在文件夹（$PSScriptRoot 不能直接做 param 默认值，需在 param 后赋值）
if (-not $TargetRoot) { $TargetRoot = $PSScriptRoot }
if (-not $TargetRoot) { throw "无法确定脚本所在目录，请通过 -TargetRoot 显式指定" }
$ManifestFile = Join-Path $TargetRoot "_symlink-migration.json"

# ── 辅助函数 ────────────────────────────────────────────────────────────

function Get-PathTail {
    # 从源路径取末尾 N 段（不含盘符）
    # 例：D:\Works\github\sdk，N=1 → sdk；N=2 → github\sdk
    param([string]$Path, [int]$Levels)
    $p = $Path.TrimEnd('\', '/')
    if ($p -match '^([A-Za-z]:)[\\/](.*)') { $p = $Matches[2] }
    elseif ($p -match '^[\\/](.*)')        { $p = $Matches[1] }
    $segs = $p -split '[\\/]' | Where-Object { $_ }
    if ($segs.Count -eq 0) { throw "无法解析路径段：$Path" }
    if ($Levels -gt $segs.Count) {
        throw "KeepLevels ($Levels) 大于路径段数 ($($segs.Count))：$Path"
    }
    return ($segs[-$Levels..-1] -join '\')
}

function Expand-PathString {
    # 展开路径字符串中的环境变量（支持 %VAR% 与 $env:VAR）
    # 不存在的变量保留原样
    param([string]$Path)
    if (-not $Path) { return $Path }
    # 1. %VAR% 风格（cmd）
    $result = [System.Environment]::ExpandEnvironmentVariables($Path)
    # 2. $env:VAR / ${env:VAR} 风格（PowerShell）
    $result = $ExecutionContext.SessionState.InvokeCommand.ExpandString($result)
    return $result
}

# 展开路径相关参数中的环境变量（支持 %VAR% 与 $env:VAR）
# 在所有路径使用前统一展开，避免 manifest 写入未展开的字面量
foreach ($p in 'Source','TargetRoot','TargetPath','OldSourceRoot','NewSourceRoot','OldTargetRoot','NewTargetRoot') {
    if ($PSBoundParameters.ContainsKey($p) -and (Get-Variable -Name $p -Scope 0).Value) {
        Set-Variable -Name $p -Value (Expand-PathString -Path (Get-Variable -Name $p -Scope 0).Value)
    }
}
# TargetRoot 是从 env 兜底过的，也走一遍
$TargetRoot = Expand-PathString -Path $TargetRoot
$ManifestFile = Join-Path $TargetRoot "_symlink-migration.json"

function Read-Manifest {
    if (-not (Test-Path $ManifestFile)) { return @() }
    try {
        $content = Get-Content $ManifestFile -Raw -Encoding UTF8 | ConvertFrom-Json
    } catch {
        throw "manifest 解析失败：$ManifestFile — $($_.Exception.Message)"
    }
    if (-not $content) { return @() }
    return @($content)
}

function Write-Manifest {
    param([array]$Records)
    $dir = Split-Path $ManifestFile -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $tmp = "$ManifestFile.tmp"
    if ($Records.Count -eq 0) {
        "[]" | Set-Content -Path $tmp -Encoding UTF8
    } else {
        $Records | ConvertTo-Json -Depth 5 | Set-Content -Path $tmp -Encoding UTF8
    }
    Move-Item $tmp $ManifestFile -Force
}

function Replace-Prefix {
    # 若 $Path 以 $OldRoot 开头，则替换为 $NewRoot；否则原样返回
    param([string]$Path, [string]$OldRoot, [string]$NewRoot)
    if (-not $OldRoot -or -not $NewRoot) { return $Path }
    $old = $OldRoot.TrimEnd('\', '/')
    if ($Path -like "$old\*" -or $Path -eq $old) {
        $rest = $Path.Substring($old.Length).TrimStart('\', '/')
        if ($rest) { return (Join-Path $NewRoot.TrimEnd('\', '/') $rest) }
        else       { return $NewRoot.TrimEnd('\', '/') }
    }
    return $Path
}

function New-DirectoryLink {
    # 不带 -Force：源路径必须不存在（与 cmd /c mklink 一致）
    # 已存在的情况由调用方在调本函数前用 Remove-Item 处理
    param([string]$LinkPath, [string]$TargetPath, [string]$Type)
    switch ($Type) {
        "Junction"     { New-Item -ItemType Junction     -Path $LinkPath -Target $TargetPath | Out-Null }
        "SymbolicLink" { New-Item -ItemType SymbolicLink -Path $LinkPath -Target $TargetPath | Out-Null }
    }
}

function Finish-LinkCreate {
    # 创建 link + 写 manifest；正常迁移流程和 -CreateEmptyLink 流程都走这里
    param(
        [string]$Source,
        [string]$TargetPath,
        [string]$LinkType,
        [string]$ManifestFile,
        [switch]$WhatIf
    )
    if ($WhatIf) {
        Write-Host "  [WhatIf] 将创建 $LinkType 链接：$Source → $TargetPath" -ForegroundColor Yellow
        return
    }
    # Junction 的目标必须实际存在（PowerShell New-Item 校验；NTFS 自身无此限制）
    if (-not (Test-Path $TargetPath)) {
        New-Item -ItemType Directory -Path $TargetPath -Force | Out-Null
    }
    New-DirectoryLink -LinkPath $Source -TargetPath $TargetPath -Type $LinkType
    Write-Host "  已创建 $LinkType：$Source → $TargetPath" -ForegroundColor Green

    # 追加 manifest（按 Source 去重）
    $records = @(Read-Manifest | Where-Object { $_.Source -ne $Source })
    $records += [pscustomobject]@{
        Source     = $Source
        Target     = $TargetPath
        KeepLevels = $KeepLevels
        LinkType   = $LinkType
        CreatedAt  = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
    }
    Write-Manifest -Records $records
    Write-Host "  已记录到 manifest：$ManifestFile" -ForegroundColor Green
}

function Test-DangerousPath {
    # 检测路径是否为用户配置文件根目录或包含 NTUSER.DAT
    # 这类路径迁移会因系统文件占用而失败，或导致 profile 损坏
    param([string]$Path)
    $p = $Path.TrimEnd('\', '/')
    # 1. 路径形如 <drive>:\Users\<name>（profile 根目录）
    if ($p -match '^[A-Za-z]:[\\/]Users[\\/][^\\/]+$') { return $true }
    # 2. 目录内含 NTUSER.DAT（profile 根的强信号）
    if (Test-Path (Join-Path $p 'NTUSER.DAT')) { return $true }
    return $false
}

# ── 迁移模式 ────────────────────────────────────────────────────────────

function Invoke-Migrate {
    $interactive = $false
    if (-not $Source) {
        $interactive = $true
        # 交互模式：未通过 CLI 提供 -Source 时，逐项提示输入（按 Ctrl+C 取消）
        Write-Host "交互模式（未指定 -Source）" -ForegroundColor Cyan

        # 源目录（必填，循环直至输入有效路径）
        do {
            $Source = (Read-Host "源目录").Trim()
            if (-not $Source) { continue }
            $Source = Expand-PathString -Path $Source
            if (-not (Test-Path $Source)) {
                Write-Warning "路径不存在：$Source"
                $action = (Read-Host "  [1] 重新输入  [2] 直接创建空目录并建 link  [3] 取消").Trim()
                switch ($action) {
                    "2" { $CreateEmptyLink = $true; break }
                    "3" { Write-Host "已取消。"; return }
                    default { $Source = $null; continue }
                }
            }
        } until ($Source)

        # 保留末尾层级（默认 1）
        $klInput = (Read-Host "保留末尾层级 [1]").Trim()
        if ($klInput) {
            $kl = 0
            if ([int]::TryParse($klInput, [ref]$kl) -and $kl -ge 1 -and $kl -le 32) {
                $KeepLevels = $kl
            } else {
                Write-Warning "层级数无效（需 1-32），使用默认值 1"
            }
        }

        # 自定义目标路径（可选；回车跳过则使用默认拼接）
        $tpInput = (Read-Host "自定义目标路径（回车跳过）").Trim()
        if ($tpInput) { $TargetPath = Expand-PathString -Path $tpInput }

        # 链接类型（序号选择，默认 1=Junction）
        Write-Host ""
        Write-Host "链接类型：" -ForegroundColor Cyan
        Write-Host "  [1] Junction（目录联接）"
        Write-Host "      无需管理员权限或开发者模式；仅限本地目录；可跨盘符；不能指向网络路径。"
        Write-Host "  [2] SymbolicLink（符号链接）"
        Write-Host "      需要管理员权限或开启开发者模式；可指向本地或网络路径；更通用。"
        $ltInput = (Read-Host "请选择 [1]").Trim()
        if ($ltInput -eq "2") {
            $LinkType = "SymbolicLink"
        } elseif ($ltInput -ne "" -and $ltInput -ne "1") {
            Write-Warning "选择无效（请输入 1 或 2），使用默认值 Junction"
        }
    }
    if (-not (Test-Path $Source)) {
        # 计算 targetPath 以便 WhatIf 也能预览
        $tail = Get-PathTail -Path $Source -Levels $KeepLevels
        if ($TargetPath) { $targetPath = $TargetPath.TrimEnd('\', '/') }
        else             { $targetPath = Join-Path $TargetRoot $tail }
        if ($WhatIf) {
            Write-Host "  [WhatIf] 源不存在将创建空目录并建立 $LinkType 链接：$Source → $targetPath" -ForegroundColor Yellow
            return
        }
        if ($CreateEmptyLink) {
            Write-Host "源不存在，按 -CreateEmptyLink 直接创建空目录并建 link：$Source" -ForegroundColor Yellow
            # 直接在源位置建 Junction（不预先建源目录；Junction 本身就是源）
            # 同时确保目标的父级目录存在（junction 目标本身由 New-Item 自动建，但中间层不会）
            $targetParent = Split-Path $targetPath -Parent
            if ($targetParent -and -not (Test-Path $targetParent)) {
                New-Item -ItemType Directory -Path $targetParent -Force | Out-Null
            }
            Finish-LinkCreate -Source $Source -TargetPath $targetPath -LinkType $LinkType -ManifestFile $ManifestFile
            return
        }
        Write-Error "源目录不存在：$Source（用 -CreateEmptyLink 可直接创建空目录并建 link）"
        return
    }
    $Source = (Resolve-Path $Source).Path.TrimEnd('\', '/')

    # 危险路径保护：用户配置文件根目录或含 NTUSER.DAT
    if (Test-DangerousPath -Path $Source) {
        if ($Force) {
            Write-Warning "强制迁移危险路径：$Source"
        } elseif ($interactive) {
            Write-Warning "'$Source' 是用户配置文件根目录或含 NTUSER.DAT，迁移可能失败或损坏系统。"
            $confirm = (Read-Host "确定强制迁移？输入 yes 继续，其他取消").Trim()
            if ($confirm -ne "yes") { Write-Host "已取消。"; return }
        } else {
            Write-Error "拒绝迁移：'$Source' 是用户配置文件根目录或含 NTUSER.DAT（系统文件占用，会失败或损坏）。加 -Force 强制执行。"
            return
        }
    }

    $item = Get-Item $Source -Force
    if ($item.LinkType) {
        Write-Warning "跳过：源已是 $($item.LinkType) 链接：$Source"
        return
    }
    if (-not $item.PSIsContainer) {
        Write-Error "源不是目录：$Source"
        return
    }

    $tail = Get-PathTail -Path $Source -Levels $KeepLevels
    if ($TargetPath) {
        # 自定义目标路径优先；-TargetRoot 失效（不被使用）
        $targetPath = $TargetPath.TrimEnd('\', '/')
    } else {
        $targetPath = Join-Path $TargetRoot $tail
    }

    # 防止源/目标互相包含导致递归
    if ($targetPath -like "$Source\*" -or $Source -like "$targetPath\*") {
        Write-Error "源与目标路径存在包含关系，可能导致递归：$Source / $targetPath"
        return
    }

    if (Test-Path $targetPath) {
        Write-Warning "跳过：目标已存在（避免覆盖数据）：$targetPath"
        return
    }

    Write-Host "迁移：$Source  →  $targetPath"
    Write-Host "  保留层级：$KeepLevels   链接类型：$LinkType"

    if ($WhatIf) {
        Write-Host "  [WhatIf] 将移动数据并创建 $LinkType 链接" -ForegroundColor Yellow
        return
    }

    # 确保目标父级存在
    $targetParent = Split-Path $targetPath -Parent
    if (-not (Test-Path $targetParent)) {
        New-Item -ItemType Directory -Path $targetParent -Force | Out-Null
    }

    # 复制数据：先 COPY，成功后再删源；失败时回滚目标，避免半移动状态
    # /R:1 /W:1：被占用文件重试 1 次后跳过（robocopy 默认重试 100 万次会挂起数日）
    Write-Host "  复制数据中..."
    $robocopyOut = & robocopy $Source $targetPath /E /R:1 /W:1 /NFL /NDL /NJH /NJS /NC /NS /NP 2>&1
    $rc = $LASTEXITCODE
    if ($rc -ge 8) {
        # 显示被占用的文件（robocopy 的 ERROR 行含完整路径）
        $errLines = @($robocopyOut | Where-Object { "$_" -match '错误|ERROR|拒绝访问|being used|cannot access|重试' })
        if ($errLines.Count -gt 0) {
            Write-Host "  以下文件可能被占用，导致复制失败：" -ForegroundColor Yellow
            $errLines | Select-Object -First 20 | ForEach-Object { Write-Host "    $_" -ForegroundColor Yellow }
        }
        # 回滚目标（源保持完整，关闭占用程序后可直接重试）
        if (Test-Path $targetPath) { Remove-Item $targetPath -Recurse -Force -ErrorAction SilentlyContinue }
        Write-Error "robocopy 复制失败 (exit=$rc)：$Source → $targetPath（关闭占用上述文件的程序后重试）"
    }

    # 复制成功：删除源目录（随后替换为 link）
    if (Test-Path $Source) { Remove-Item $Source -Recurse -Force }

    Finish-LinkCreate -Source $Source -TargetPath $targetPath -LinkType $LinkType -ManifestFile $ManifestFile
}

# ── 挂载模式 ────────────────────────────────────────────────────────────

function Invoke-Mount {
    $records = Read-Manifest
    if ($records.Count -eq 0) {
        Write-Host "manifest 为空或不存在：$ManifestFile"
        return
    }

    Write-Host "重建 $($records.Count) 个链接..." -ForegroundColor Cyan
    $ok = 0; $skip = 0; $fail = 0

    foreach ($r in $records) {
        $src = Replace-Prefix -Path $r.Source -OldRoot $OldSourceRoot -NewRoot $NewSourceRoot
        $tgt = Replace-Prefix -Path $r.Target -OldRoot $OldTargetRoot -NewRoot $NewTargetRoot

        Write-Host ""
        Write-Host "[$($r.LinkType)] $src  →  $tgt"

        if (-not (Test-Path $tgt)) {
            if ($CreateMissing) {
                Write-Host "  目标不存在（-CreateMissing），将创建空目录：$tgt" -ForegroundColor Yellow
            } else {
                Write-Warning "  跳过：目标数据不存在：$tgt"
                $skip++; continue
            }
        }
        if (Test-Path $src) {
            $srcItem = Get-Item $src -Force
            if ($srcItem.LinkType) {
                Write-Warning "  跳过：源已是链接：$src"
            } else {
                Write-Warning "  跳过：源已存在且非链接（避免覆盖数据）：$src"
            }
            $skip++; continue
        }
        $srcParent = Split-Path $src -Parent
        if (-not (Test-Path $srcParent)) {
            if ($CreateMissing) {
                Write-Host "  源父级不存在（-CreateMissing），将创建：$srcParent" -ForegroundColor Yellow
            } else {
                Write-Warning "  跳过：源父级不存在：$srcParent"
                $skip++; continue
            }
        }

        if ($WhatIf) {
            Write-Host "  [WhatIf] 将创建 $($r.LinkType)" -ForegroundColor Yellow
            $ok++; continue
        }

        try {
            if (-not (Test-Path $tgt)) {
                $tgtParent = Split-Path $tgt -Parent
                if ($tgtParent -and -not (Test-Path $tgtParent)) {
                    New-Item -ItemType Directory -Path $tgtParent -Force | Out-Null
                }
                New-Item -ItemType Directory -Path $tgt -Force | Out-Null
            }
            if (-not (Test-Path $srcParent)) {
                New-Item -ItemType Directory -Path $srcParent -Force | Out-Null
            }
            New-DirectoryLink -LinkPath $src -TargetPath $tgt -Type $r.LinkType
            Write-Host "  已创建" -ForegroundColor Green
            $ok++
        } catch {
            Write-Warning "  失败：$($_.Exception.Message)"
            $fail++
        }
    }

    Write-Host ""
    Write-Host "完成：成功 $ok / 跳过 $skip / 失败 $fail" -ForegroundColor Cyan
}

# ── 主流程 ──────────────────────────────────────────────────────────────

if (-not (Test-Path $TargetRoot)) {
    New-Item -ItemType Directory -Path $TargetRoot -Force | Out-Null
}

if ($Mount) {
    Invoke-Mount
} else {
    Invoke-Migrate
}
