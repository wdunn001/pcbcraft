<#
.SYNOPSIS
  Build and package PCBCraft for Windows x86_64.

.DESCRIPTION
  Produces, in $env:DIST (default: dist):
    pcbcraft-<dist-tag>-windows-x86_64.msi
    pcbcraft-<dist-tag>-windows-x86_64-portable.zip
    PCBCraft-<dist-tag>-windows-x86_64.exe   plain app download

  Binaries link the C runtime statically (+crt-static). The icon comes from
  $env:PCBCRAFT_ICONS (pcbcraft.ico) or assets/icons/app/windows/exports/.
  Nothing is signed.

  Needs: Rust (MSVC x86_64 toolchain) and WiX v5:
    dotnet tool install --global wix --version 5.0.2
    wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2

  Adapted from SolidCraft packaging (bherbruck/solidcraft; MIT OR Apache-2.0; see ATTRIBUTION.md).

.EXAMPLE
  pwsh packaging/windows/package.ps1
  pwsh packaging/windows/package.ps1 -SkipBuild
#>
param(
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Arch = 'x86_64'
$Target = 'x86_64-pc-windows-msvc'

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

$DistTag = $env:PCBCRAFT_DIST_TAG
if (-not $DistTag) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $DistTag = "v$($Matches[1])"; break }
  }
}
if (-not $DistTag) { throw 'could not read version from Cargo.toml or PCBCRAFT_DIST_TAG' }
$Version = ($DistTag -replace '^v', '')
$MsiVersion = ($Version -split '-')[0]

$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "PCBCraft $DistTag for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  $flagVar = 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p pcbcraft -p pcbcraft-cli --target $Target }
}
$Bin = Join-Path $TargetDir "$Target\release"

function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = 0x8664
foreach ($check in @(@('pcbcraft.exe', 2), @('pcbcraft-cli.exe', 3))) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne $Machine) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}
$Stage = Join-Path $TargetDir "windows-package\$Arch"
& (Join-Path $PSScriptRoot 'check-icon.ps1') -Path (Join-Path $Bin 'pcbcraft.exe')
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'pcbcraft.exe'), (Join-Path $Bin 'pcbcraft-cli.exe') $Stage
foreach ($f in 'LICENSE', 'NOTICE', 'ATTRIBUTION.md') { Copy-Item (Join-Path $Root $f) $Stage }

$Icon = if ($env:PCBCRAFT_ICONS) { Join-Path $env:PCBCRAFT_ICONS 'pcbcraft.ico' } else { Join-Path $Root 'assets/icons/app/windows/exports/pcbcraft.ico' }
if (-not (Test-Path $Icon)) { throw "Missing package icon: $Icon" }

$Msi = Join-Path $Dist "pcbcraft-$DistTag-windows-$Arch.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $Root 'assets/platforms/windows/pcbcraft.wxs') -arch x64 `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -culture en-US -loc (Join-Path $Root 'assets/platforms/windows/pcbcraft.en-us.wxl') `
    -d "Version=$MsiVersion" -d "DisplayVersion=$Version" -d "BinDir=$Stage" -d "IconPath=$Icon" `
    -o $Msi
}
Invoke-Native 'MSI shortcut icon validation (ICE50)' {
  wix msi validate $Msi -ice ICE50 -intermediateFolder (Join-Path $Stage 'msi-validation')
}
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))

$Portable = Join-Path $TargetDir "windows-package\pcbcraft-$DistTag-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
foreach ($f in 'README.md', 'LICENSE', 'NOTICE', 'ATTRIBUTION.md') { Copy-Item (Join-Path $Root $f) $Portable }
$Zip = Join-Path $Dist "pcbcraft-$DistTag-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

$Exe = Join-Path $Dist "PCBCraft-$DistTag-windows-$Arch.exe"
Copy-Item (Join-Path $Stage 'pcbcraft.exe') $Exe
$Outputs = @($Msi, $Zip, $Exe)

Invoke-Native 'pcbcraft-cli --version' { & (Join-Path $Stage 'pcbcraft-cli.exe') --version }
Get-Item $Outputs | Format-Table Name, Length
