$ErrorActionPreference = "Stop"

$Repo = if ($env:MYOPENPANELS_INSTALL_REPO) { $env:MYOPENPANELS_INSTALL_REPO } else { "mooqii/OpenPanels" }
$ManifestUrl = if ($env:MYOPENPANELS_INSTALL_MANIFEST_URL) {
  $env:MYOPENPANELS_INSTALL_MANIFEST_URL
} elseif ($env:MYOPENPANELS_UPDATE_MANIFEST_URL) {
  $env:MYOPENPANELS_UPDATE_MANIFEST_URL
} else {
  "https://github.com/$Repo/releases/latest/download/myopenpanels-manifest.json"
}
$InstallDir = if ($env:MYOPENPANELS_INSTALL_DIR) {
  $env:MYOPENPANELS_INSTALL_DIR
} else {
  Join-Path $HOME ".local\bin"
}

function Fail($Message) {
  throw "myopenpanels install failed: $Message"
}

function CurrentTarget {
  if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    Fail "install-myopenpanels.ps1 only supports Windows. Use install-myopenpanels.sh on macOS/Linux."
  }
  switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { return "x86_64-pc-windows-msvc" }
    default { Fail "unsupported Windows architecture: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" }
  }
}

function DownloadFile($Url, $OutFile) {
  Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
}

$Target = CurrentTarget
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("myopenpanels-install-" + [System.Guid]::NewGuid().ToString("N"))
$ExtractDir = Join-Path $TempDir "extract"
$ArchivePath = Join-Path $TempDir "myopenpanels.zip"

New-Item -ItemType Directory -Force -Path $TempDir, $ExtractDir | Out-Null

try {
  Write-Host "Installing myopenpanels for $Target"
  $Manifest = Invoke-RestMethod -Uri $ManifestUrl
  $Asset = $Manifest.assets.PSObject.Properties[$Target].Value
  if (-not $Asset -or -not $Asset.url) {
    Fail "no release asset for $Target in $ManifestUrl"
  }
  if (-not $Asset.sha256) {
    Fail "manifest asset for $Target has no sha256"
  }

  DownloadFile $Asset.url $ArchivePath
  $ActualSha = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()
  $ExpectedSha = [string]$Asset.sha256
  if ($ActualSha -ne $ExpectedSha.ToLowerInvariant()) {
    Fail "checksum mismatch for downloaded archive"
  }
  if ($Asset.size -and (Get-Item $ArchivePath).Length -ne [int64]$Asset.size) {
    Fail "size mismatch for downloaded archive"
  }

  Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractDir -Force
  $ExtractedBinary = Get-ChildItem -Path $ExtractDir -Recurse -File -Filter "myopenpanels.exe" | Select-Object -First 1
  if (-not $ExtractedBinary) {
    Fail "release archive does not contain myopenpanels.exe"
  }

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  $InstallPath = Join-Path $InstallDir "myopenpanels.exe"
  Copy-Item -LiteralPath $ExtractedBinary.FullName -Destination $InstallPath -Force

  Write-Host "Installed myopenpanels to $InstallPath"
  & $InstallPath --version

  $PathEntries = ($env:PATH -split ";") | ForEach-Object { $_.TrimEnd("\") }
  if ($PathEntries -notcontains $InstallDir.TrimEnd("\")) {
    Write-Host ""
    Write-Host "$InstallDir is not currently on PATH."
    Write-Host "Add it to PATH, then restart your terminal:"
    Write-Host "  $InstallDir"
  }
} finally {
  Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
