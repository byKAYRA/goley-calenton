$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$env:NMRunEnv_VER = '0'
$env:NMRunEnv_ENUM = 'NMRunEnv_DATA_1'
$env:NMRunEnv_DATA_1 = 'fc2329727b4856f29186165dcc4dfe822fa1c2959e48f21aa738aedfb42c1c2ccaaafb29cb7efa641dbe726dd07a810241ac4b6b5edb2c305d473a0f5f8b6386206d0f4b985160b36d662872e5537ea86f685615565cf9bd3739018742b38d23'

$clientExe = if ($args[0]) { $args[0] } else { 'C:\Joygame\Goley\BinaryTr\BinaryTr.exe' }
$shimDir = Join-Path $PSScriptRoot "target\i686-pc-windows-msvc\release"

if (-not (Test-Path "$shimDir\goley-boot.exe") -or -not (Test-Path "$shimDir\goley_shim.dll")) {
    Write-Host "Building goley-boot and goley-shim (i686 release)..."
    cargo build --release --target i686-pc-windows-msvc
}

Write-Host "Launching Goley client with goley-boot injector..."
Start-Process -FilePath "$shimDir\goley-boot.exe" `
  -ArgumentList @(
    "run",
    "--client", $clientExe,
    "--region", "TRAuth",
    "--runparam-key", "NMRP20260816LOCALKEY0001",
    "--oep-rva", "0x009374DB",
    "--late-inject-ms", "3000",
    "--shim", "$shimDir\goley_shim.dll",
    "--patches", "$PSScriptRoot\crates\goley-shim\patches\patches.toml",
    "--entry", "127.0.0.1:2270",
    "--timeout", "150",
    "-vv"
  ) `
  -WorkingDirectory $PSScriptRoot

Write-Host "goley-boot launched successfully."
