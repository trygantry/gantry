[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Gantry\bin"),
    [string]$DownloadBase = "",
    [switch]$NoModifyPath
)

$ErrorActionPreference = "Stop"
$repository = "https://github.com/trygantry/gantry-distribution"
$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()

if ($architecture -ne "X64") {
    throw "Gantry currently publishes the Windows CLI for x86_64 only; detected '$architecture'."
}

$asset = "gantry-windows-x86_64.exe"
if (-not $DownloadBase) {
    if ($Version -eq "latest") {
        $DownloadBase = "$repository/releases/latest/download"
    } else {
        $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
        $DownloadBase = "$repository/releases/download/$tag"
    }
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gantry-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempDir | Out-Null

try {
    $binaryPath = Join-Path $tempDir $asset
    $checksumPath = "$binaryPath.sha256"
    Invoke-WebRequest -UseBasicParsing -Uri "$DownloadBase/$asset" -OutFile $binaryPath
    Invoke-WebRequest -UseBasicParsing -Uri "$DownloadBase/$asset.sha256" -OutFile $checksumPath

    $expected = ((Get-Content -Raw $checksumPath).Trim() -split '\s+')[0].ToLowerInvariant()
    if ($expected -notmatch '^[0-9a-f]{64}$') {
        throw "The published checksum for $asset is malformed."
    }
    $actual = (Get-FileHash -Algorithm SHA256 $binaryPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "Checksum verification failed for $asset; Gantry was not installed."
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $target = Join-Path $InstallDir "gantry.exe"
    Copy-Item -Force $binaryPath $target

    if (-not $NoModifyPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $parts = @($userPath -split ';' | Where-Object { $_ })
        if ($parts -notcontains $InstallDir) {
            $newUserPath = (@($parts) + $InstallDir) -join ';'
            [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
        }
        if (($env:Path -split ';') -notcontains $InstallDir) {
            $env:Path = "$InstallDir;$env:Path"
        }
    }

    Write-Host "Installed Gantry at $target"
    & $target --version
    if ($NoModifyPath) {
        Write-Host "Add $InstallDir to your user PATH to run gantry from any PowerShell window."
    }
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tempDir
}
