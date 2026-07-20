# One-line install (run in PowerShell):
#   irm https://raw.githubusercontent.com/lidongbei/sdk/main/install.ps1 | iex
#
# Or with a custom install directory:
#   $env:SDK_INSTALL_DIR = "C:\tools\sdk"; irm ... | iex

$ErrorActionPreference = "Stop"

$Repo       = "lidongbei/sdk"
$InstallDir = if ($env:SDK_INSTALL_DIR) { $env:SDK_INSTALL_DIR } `
              else { Join-Path $env:USERPROFILE ".sdk\bin" }

# ── Detect architecture ────────────────────────────────────────────────────
$CpuArch = $env:PROCESSOR_ARCHITECTURE
$ArchTag  = if ($CpuArch -eq "ARM64") { "aarch64" } else { "x86_64" }
$Target   = "$ArchTag-pc-windows-msvc"

# ── Fetch latest release tag ───────────────────────────────────────────────
# Use the releases/latest 302 redirect to get the tag without consuming the
# GitHub API rate limit (60/hour per IP for unauthenticated requests).
# Falls back to the API (with GITHUB_TOKEN if set) if the redirect fails.
Write-Host "Fetching latest release..."
$Tag = $null
try {
    $Req = [System.Net.HttpWebRequest]::Create("https://github.com/$Repo/releases/latest")
    $Req.AllowAutoRedirect = $false
    $Req.UserAgent = "sdk-installer"
    $Resp = $Req.GetResponse()
    $Redirect = $Resp.GetResponseHeader("Location")
    $Resp.Close()
    if ($Redirect -match '/releases/tag/(.+)$') { $Tag = $Matches[1] }
} catch {
    $Headers = @{ "User-Agent" = "sdk-installer" }
    if ($env:GITHUB_TOKEN) { $Headers["Authorization"] = "Bearer $env:GITHUB_TOKEN" }
    $Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers $Headers
    $Tag = $Release.tag_name
}
if (-not $Tag) { Write-Error "Could not determine latest release tag"; exit 1 }

Write-Host "Installing sdk $Tag ($Target)..."

# ── Download & extract ─────────────────────────────────────────────────────
$Archive  = "sdk-$Tag-$Target.zip"
$Url      = "https://github.com/$Repo/releases/download/$Tag/$Archive"
$Tmp      = Join-Path $env:TEMP "sdk-install-$([System.Guid]::NewGuid())"

New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $ArchivePath = Join-Path $Tmp $Archive
    Invoke-WebRequest $Url -OutFile $ArchivePath -UseBasicParsing
    Expand-Archive $ArchivePath -DestinationPath $Tmp

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item "$Tmp\sdk-$Tag-$Target\sdk.exe" "$InstallDir\sdk.exe" -Force

    # ── Add to user PATH if not already present ────────────────────────────
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$UserPath", "User")
        Write-Host "  Added $InstallDir to user PATH"
    }

    # ── Configure PowerShell profile ──────────────────────────────────────
    $HookLine = 'Invoke-Expression (& sdk hook powershell | Out-String)'
    $ProfileFile = $PROFILE.CurrentUserCurrentHost
    $ProfileWritten = $false
    if (-not (Test-Path $ProfileFile) -or -not (Select-String -Path $ProfileFile -Pattern 'sdk hook powershell' -Quiet)) {
        $ProfileDir = Split-Path $ProfileFile -Parent
        if (-not (Test-Path $ProfileDir)) { New-Item -ItemType Directory -Path $ProfileDir -Force | Out-Null }
        if (-not (Test-Path $ProfileFile)) { New-Item -ItemType File -Path $ProfileFile -Force | Out-Null }
        Add-Content -Path $ProfileFile -Value "`n$HookLine"
        $ProfileWritten = $true
    }

    Write-Host ""
    Write-Host "✓ sdk $Tag installed → $InstallDir\sdk.exe" -ForegroundColor Green
    Write-Host ""
    if ($ProfileWritten) {
        Write-Host "  ✓ Shell hook written to: $ProfileFile" -ForegroundColor Green
        Write-Host ""
        Write-Host "▸ Restart your terminal (or run '. `$PROFILE') for changes to take effect."
    } else {
        Write-Host "▸ Shell profile already configured: $ProfileFile"
    }
    Write-Host ""
    Write-Host "  Restart your terminal for PATH changes to take effect."
    Write-Host ""
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
