$ErrorActionPreference = 'Stop'
$binary = Join-Path $PSScriptRoot '../target/release/herdr-command-shelf.exe'
& $binary install-keybindings
exit $LASTEXITCODE
