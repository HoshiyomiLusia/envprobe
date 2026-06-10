# Install envprobe from GitHub Releases on Windows.
#
# Usage:
#   irm https://raw.githubusercontent.com/HoshiyomiLusia/envprobe/main/install.ps1 | iex
#
# Environment variables:
#   ENVPROBE_VERSION      Specific version to install (e.g. v0.1.0). Default: latest.
#   ENVPROBE_INSTALL_DIR  Override install directory.
#                         Default: %LOCALAPPDATA%\Programs\envprobe

#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repo = 'HoshiyomiLusia/envprobe'
$githubUrl = "https://github.com/$repo"
$apiUrl = "https://api.github.com/repos/$repo"

function Resolve-EnvprobeVersion {
    if ($env:ENVPROBE_VERSION) {
        if ($env:ENVPROBE_VERSION -match '^v') { return $env:ENVPROBE_VERSION }
        return "v$($env:ENVPROBE_VERSION)"
    }
    $headers = @{ 'User-Agent' = 'envprobe-install'; 'Accept' = 'application/vnd.github+json' }
    $release = Invoke-RestMethod -Uri "$apiUrl/releases/latest" -Headers $headers
    if (-not $release.tag_name) {
        throw "could not resolve the latest envprobe version from $apiUrl/releases/latest"
    }
    return $release.tag_name
}

function Resolve-InstallDir {
    if ($env:ENVPROBE_INSTALL_DIR) { return $env:ENVPROBE_INSTALL_DIR }
    return (Join-Path $env:LOCALAPPDATA 'Programs\envprobe')
}

function Add-ToUserPath {
    param([string]$Dir)
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()
    if ($userPath) { $parts = $userPath -split ';' | Where-Object { $_ -ne '' } }
    if ($parts -notcontains $Dir) {
        $newPath = (@($parts) + $Dir) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        return $true
    }
    return $false
}

# Only x86_64 Windows binaries are published; they run on ARM via emulation.
$arch = 'x86_64'
$version = Resolve-EnvprobeVersion
$installDir = Resolve-InstallDir
$name = "envprobe-$version-windows-$arch"
$asset = "$name.zip"
$assetUrl = "$githubUrl/releases/download/$version/$asset"
$checksumUrl = "$assetUrl.sha256"

$tmp = Join-Path $env:TEMP "envprobe-install-$PID"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $asset
    Write-Host "Downloading $asset ..."
    Invoke-WebRequest -Uri $assetUrl -OutFile $zip -UseBasicParsing

    Write-Host "Verifying SHA-256 ..."
    $checksumText = (Invoke-WebRequest -Uri $checksumUrl -UseBasicParsing).Content
    $expected = (($checksumText.Trim()) -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) {
        throw "checksum mismatch for $asset (expected $expected, got $actual)"
    }

    Write-Host "Extracting ..."
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $exe = Join-Path $tmp "$name\envprobe.exe"
    if (-not (Test-Path $exe)) {
        throw "extracted archive does not contain $name\envprobe.exe"
    }

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item -Path $exe -Destination (Join-Path $installDir 'envprobe.exe') -Force

    Write-Host ""
    Write-Host "Installed envprobe $version to $installDir\envprobe.exe"
    if (Add-ToUserPath $installDir) {
        Write-Host "Added $installDir to your user PATH. Open a new terminal, then run: envprobe"
    } else {
        Write-Host "Run: envprobe"
    }
    Write-Host "To update later, re-run this command."
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
