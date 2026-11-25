# Runs under Windows PowerShell 5.1 too. Downloads are mocked; Cargo must not run.
$ErrorActionPreference = 'Stop'
$Fixture = Join-Path ([IO.Path]::GetTempPath()) ('shelf-install-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $Fixture 'scripts') -Force | Out-Null
Copy-Item (Join-Path $PSScriptRoot '../scripts/install.ps1') (Join-Path $Fixture 'scripts/install.ps1')
Copy-Item (Join-Path $PSScriptRoot '../herdr-plugin.toml') (Join-Path $Fixture 'herdr-plugin.toml')
$global:ShelfInstallerTestCorrupt = $false
function cargo { throw 'Installer must not invoke Cargo.' }
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    $ExpectedUrl = "https://github.com/kormic911/herdr-command-shelf/releases/download/v$Version/herdr-command-shelf-v$Version-$ExpectedTarget.exe"
    if ($Uri -notin @($ExpectedUrl, "$ExpectedUrl.sha256")) { throw "Unexpected URL: $Uri" }
    if ($Uri.EndsWith('.sha256')) {
        $Binary = Join-Path (Split-Path $OutFile) ((Split-Path $Uri -Leaf) -replace '\.sha256$', '')
        $Hash = (Get-FileHash $Binary -Algorithm SHA256).Hash
        if ($global:ShelfInstallerTestCorrupt) { $Hash = '0' * 64 }
        [IO.File]::WriteAllText($OutFile, $Hash)
    } else { [IO.File]::WriteAllText($OutFile, 'downloaded fixture') }
}
function Get-ItemPropertyValue {
    param($LiteralPath, $Name)
    if ($LiteralPath -ne 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -or $Name -ne 'PROCESSOR_ARCHITECTURE') { throw 'Unexpected registry query.' }
    return $MachineArch
}
try {
    $Installed = Join-Path $Fixture 'target/release/herdr-command-shelf.exe'
    foreach ($MachineArch in 'AMD64', 'ARM64') {
        $ExpectedTarget = if ($MachineArch -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
        & (Join-Path $Fixture 'scripts/install.ps1')
        if ((Get-Content $Installed -Raw) -ne 'downloaded fixture') { throw "Binary not installed for $MachineArch." }
    }
    [IO.File]::WriteAllText($Installed, 'keep existing binary')
    $global:ShelfInstallerTestCorrupt = $true
    $Rejected = $false
    try { & (Join-Path $Fixture 'scripts/install.ps1') } catch {
        if ($_ -notmatch 'checksum mismatch') { throw }
        $Rejected = $true
    }
    if (-not $Rejected -or (Get-Content $Installed -Raw) -ne 'keep existing binary') { throw 'Corruption replaced the existing binary.' }
    Write-Output 'PASS: x64 and ARM64 assets selected; no Cargo needed; corrupt download rejected; existing binary preserved.'
} finally {
    if ((Split-Path -Parent ([IO.Path]::GetFullPath($Fixture))).TrimEnd([IO.Path]::DirectorySeparatorChar) -ne ([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar)) { throw 'Invalid test directory.' }
    Remove-Item -LiteralPath $Fixture -Recurse -Force
}
