$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$resourcesDir = Join-Path $repoRoot "src-tauri/resources"
$targetDll = Join-Path $resourcesDir "WebView2Loader.dll"

if (Test-Path $targetDll) {
  $existing = Get-Item $targetDll
  Write-Host "WebView2Loader.dll already exists: $($existing.FullName) ($($existing.Length) bytes)"
  exit 0
}

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
  "arm64"
} else {
  "x64"
}

$tempRoot = Join-Path $env:TEMP ("jtools-webview2-" + [Guid]::NewGuid().ToString("N"))
$pkgPath = Join-Path $tempRoot "Microsoft.Web.WebView2.nupkg"
$extractDir = Join-Path $tempRoot "pkg"

New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null

try {
  Write-Host "Downloading WebView2 runtime loader package (arch=$arch)..."
  Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2" -OutFile $pkgPath

  Expand-Archive -Path $pkgPath -DestinationPath $extractDir -Force
  $sourceDll = Join-Path $extractDir "build/native/$arch/WebView2Loader.dll"
  if (!(Test-Path $sourceDll)) {
    throw "WebView2Loader.dll not found at: $sourceDll"
  }

  New-Item -ItemType Directory -Force -Path $resourcesDir | Out-Null
  Copy-Item -Path $sourceDll -Destination $targetDll -Force
  $copied = Get-Item $targetDll
  Write-Host "Prepared WebView2Loader.dll: $($copied.FullName) ($($copied.Length) bytes)"
}
finally {
  if (Test-Path $tempRoot) {
    Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
