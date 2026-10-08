# Exercise the source module on an isolated runner, never the GUI binary.
[CmdletBinding()]
param(
    [string] $OutputDirectory = 'dist/windows-git-regression'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$workRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory, $workRoot)
if (Test-Path -LiteralPath $output) {
    throw 'The regression output directory must be fresh.'
}
New-Item -ItemType Directory -Path $output | Out-Null
$fixture = Join-Path ([IO.Path]::GetTempPath()) (
    'disktree-git-regression-' + [guid]::NewGuid()
)
New-Item -ItemType Directory -Path $fixture | Out-Null
$probe = Join-Path $fixture 'probe.exe'
$helper = Join-Path $fixture 'git.exe'
& rustc --edition 2024 -C target-feature=+crt-static (
    Join-Path $PSScriptRoot 'check-windows-git.rs'
) -o $probe
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the source-module probe.' }
& rustc --edition 2024 -C target-feature=+crt-static (
    Join-Path $PSScriptRoot 'check-git-helper.rs'
) -o $helper
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the inert helper.' }

# This environment variable is scoped to the CI step running this script.
$env:AUDIT_HELPER_MARKER = Join-Path $fixture 'helper-marker.txt'
& $probe $fixture --control |
    Tee-Object -FilePath (Join-Path $output 'control.txt')
if ($LASTEXITCODE -ne 0) { throw 'The control did not reach the helper.' }
Copy-Item -LiteralPath $env:AUDIT_HELPER_MARKER -Destination (
    Join-Path $output 'control-helper-invocations.txt'
)
Remove-Item -LiteralPath $env:AUDIT_HELPER_MARKER
& $probe $fixture | Tee-Object -FilePath (Join-Path $output 'regression.txt')
if ($LASTEXITCODE -ne 0) {
    throw 'Automatic checkout inspection executed a Git helper.'
}
if (Test-Path -LiteralPath $env:AUDIT_HELPER_MARKER) {
    throw 'The Git helper ran after the control marker was cleared.'
}
