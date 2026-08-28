#Requires -Version 5.1

$ErrorActionPreference = 'Stop'

$principal = [Security.Principal.WindowsPrincipal]::new(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
$isAdministrator = $principal.IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdministrator) {
    $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList $arguments
    exit
}

$npcapInstaller = '<ToolsPath>\GoleyRE\Wireshark-4.6.8\npcap-1.88.exe'
if (-not (Test-Path -LiteralPath $npcapInstaller)) {
    throw "Npcap installer bulunamadı: $npcapInstaller"
}

Write-Host '1/2 Npcap 1.88 kurulumu açılıyor...'
Start-Process -FilePath $npcapInstaller -Wait

Write-Host '2/2 Visual Studio 2022 Build Tools + MSVC/Windows SDK kuruluyor...'
& winget install `
    --id Microsoft.VisualStudio.2022.BuildTools `
    --exact `
    --source winget `
    --accept-source-agreements `
    --accept-package-agreements `
    --override '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'

if ($LASTEXITCODE -ne 0) {
    throw "winget çıkış kodu: $LASTEXITCODE"
}

Write-Host ''
Write-Host 'Yönetici kurulumları tamamlandı. Doğrulama:'
Get-Service -Name npcap -ErrorAction SilentlyContinue |
    Select-Object Name, Status, StartType |
    Format-Table -AutoSize

$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) {
    & $vswhere -latest -products '*' `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
}

Read-Host 'Kapatmak için Enter'
