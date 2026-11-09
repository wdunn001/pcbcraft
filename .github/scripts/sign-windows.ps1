# Sign Windows release executables with Authenticode when certificate secrets exist.
# Reads WINDOWS_CERTIFICATE (base64 PFX) and WINDOWS_CERTIFICATE_PASSWORD from the environment.
# Never prints secret values.
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE)) {
    Write-Host 'skipped, no certificate'
    exit 0
}

$triple = 'x86_64-pc-windows-msvc'
$root = Join-Path $PWD "target/$triple/release"
$exes = @(
    (Join-Path $root 'pcbcraft.exe'),
    (Join-Path $root 'pcbcraft-cli.exe')
)

foreach ($path in $exes) {
    if (-not (Test-Path -LiteralPath $path)) {
        Write-Error "missing binary $path"
    }
}

$pfxBytes = [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE)
$pfxPath = Join-Path $env:RUNNER_TEMP 'pcbcraft-codesign.pfx'
[System.IO.File]::WriteAllBytes($pfxPath, $pfxBytes)

$signtool = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.22621.0/x64/signtool.exe'
if (-not (Test-Path -LiteralPath $signtool)) {
    $signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue).Source
}
if (-not $signtool) {
    Write-Error 'signtool.exe not found'
}

$signArgs = @(
    'sign', '/fd', 'SHA256', '/f', $pfxPath,
    '/p', $env:WINDOWS_CERTIFICATE_PASSWORD,
    '/tr', 'http://timestamp.digicert.com', '/td', 'SHA256'
)
foreach ($path in $exes) {
    & $signtool @signArgs $path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & $signtool verify '/pa' $path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue
Write-Host 'Windows executables signed and verified'
