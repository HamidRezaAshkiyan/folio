$ErrorActionPreference = "Stop"

$Repo = "HamidRezaAshkiyan/folio"
$Version = if ($env:FOLIO_VERSION) { $env:FOLIO_VERSION } else { "latest" }
$Asset = "folio-windows-x64.zip"

if ($Version -eq "latest") {
    $Url = "https://github.com/$Repo/releases/latest/download/$Asset"
} else {
    $Url = "https://github.com/$Repo/releases/download/$Version/$Asset"
}

$InstallDir = if ($env:FOLIO_INSTALL_DIR) { $env:FOLIO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Folio\bin" }
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("folio-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    $ZipPath = Join-Path $TempDir $Asset
    Write-Host "Downloading $Url..."
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath

    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
    Copy-Item -Path (Join-Path $TempDir "folio.exe") -Destination (Join-Path $InstallDir "folio.exe") -Force

    # Add InstallDir to User PATH if missing
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not ($UserPath -split ";" | Where-Object { $_ -eq $InstallDir })) {
        $NewPath = if ([string]::IsNullOrWhiteSpace($UserPath)) { $InstallDir } else { "$UserPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Host "Added $InstallDir to your User PATH (restart terminal to take effect)."
    }

    Write-Host "Installed folio to $(Join-Path $InstallDir 'folio.exe')"
    Write-Host "Run 'folio --tui' or 'folio --gui' to get started."
} finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
