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
Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name
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

    Write-Host ""
    Write-Host "✓ sdk $Tag installed → $InstallDir\sdk.exe" -ForegroundColor Green
    Write-Host ""
    Write-Host "▸ Enable the shell hook (version auto-switching)."
    Write-Host "  Add the following to your PowerShell profile:`n"
    Write-Host "    Invoke-Expression (& sdk hook powershell | Out-String)"
    Write-Host ""
    Write-Host "  To find your profile path: `$PROFILE"
    Write-Host ""
    Write-Host "  Restart your terminal for PATH changes to take effect."
    Write-Host ""
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
