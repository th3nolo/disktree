# Static inspection of the exact prior candidate; never launch disktree.
[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$output = Join-Path $env:GITHUB_WORKSPACE 'dist/security-review'
$download = Join-Path $env:GITHUB_WORKSPACE 'candidate-download'
$evidence = Join-Path $env:GITHUB_WORKSPACE 'candidate-evidence'
$archives = @(Get-ChildItem -LiteralPath $download -Filter '*.zip' -File)
if ($archives.Count -ne 1) { throw 'Expected exactly one candidate archive.' }
$archive = $archives[0]
$archiveHash = (Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$sidecar = (Get-Content -LiteralPath ($archive.FullName + '.sha256') -Raw).Trim()
$sidecarPattern = '^' + $archiveHash + '\s+' + [Regex]::Escape($archive.Name) + '$'
if ($sidecar -notmatch $sidecarPattern) { throw 'Candidate ZIP checksum mismatch.' }

$expected = @(
    'BUILD-PROVENANCE.json', 'disktree.exe', 'disktree.exe.sha256',
    'LICENSE', 'README.md', 'removal-safety.md', 'windows-validation.md'
)
$payload = Join-Path $output 'payload'
New-Item -ItemType Directory -Path $payload | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive.FullName)
try {
    $entries = @($zip.Entries | ForEach-Object FullName)
    if ($entries.Count -ne $expected.Count -or
        @($entries | Select-Object -Unique).Count -ne $expected.Count) {
        throw 'Unexpected or duplicate candidate ZIP entries.'
    }
    foreach ($entry in $zip.Entries) {
        if ($expected -cnotcontains $entry.FullName) {
            throw "Unexpected ZIP entry: $($entry.FullName)"
        }
        [IO.Compression.ZipFileExtensions]::ExtractToFile(
            $entry, (Join-Path $payload $entry.FullName), $false
        )
    }
} finally { $zip.Dispose() }

$exe = Join-Path $payload 'disktree.exe'
$exeHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
$exeSidecar = (Get-Content -LiteralPath (Join-Path $payload 'disktree.exe.sha256') -Raw).Trim()
if ($exeSidecar -notmatch ('^' + $exeHash + '\s+disktree\.exe$')) {
    throw 'Executable checksum mismatch.'
}
$provenance = Get-Content -LiteralPath (Join-Path $payload 'BUILD-PROVENANCE.json') -Raw | ConvertFrom-Json
$validation = Get-Content -LiteralPath (Join-Path $evidence 'validation.json') -Raw | ConvertFrom-Json
$source = '1ba077fc267d76937594fba984c281e255d70c52'
$head = 'b0696970d8b449170d6b713f5a594886cb0018a7'
if ($provenance.source_commit -ne $source -or
    $provenance.pull_request_head_commit -ne $head -or
    $provenance.executable_sha256 -ne $exeHash -or
    $validation.source_commit -ne $source -or
    $validation.executable_sha256 -ne $exeHash -or
    $validation.archive_sha256 -ne $archiveHash -or
    $validation.result -ne 'passed' -or
    $validation.executable_scan_exit_code -ne 0 -or
    $validation.archive_scan_exit_code -ne 0 -or
    $validation.help_exit_code -ne 0) {
    throw 'Candidate and validation evidence disagree.'
}
$lockHash = (Get-FileHash -LiteralPath 'Cargo.lock' -Algorithm SHA256).Hash.ToLowerInvariant()
$scriptHash = (Get-FileHash -LiteralPath 'scripts/validate-windows.ps1' -Algorithm SHA256).Hash.ToLowerInvariant()
if ($provenance.cargo_lock_sha256 -ne $lockHash -or
    $provenance.validation_script_sha256 -ne $scriptHash) {
    throw 'The reviewed lockfile or original validator differs from build evidence.'
}
Copy-Item -LiteralPath (Join-Path $evidence 'validation.json') -Destination $output
Copy-Item -LiteralPath (Join-Path $payload 'BUILD-PROVENANCE.json') -Destination $output

$bytes = [IO.File]::ReadAllBytes($exe)
if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Missing DOS signature.' }
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ([BitConverter]::ToUInt32($bytes, $pe) -ne 0x00004550) { throw 'Missing PE signature.' }
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$optional = $pe + 24
if ([BitConverter]::ToUInt16($bytes, $optional) -ne 0x20b -or $machine -ne 0x8664) {
    throw 'Expected an x64 PE32+ candidate.'
}
$characteristics = [BitConverter]::ToUInt16($bytes, $optional + 70)
$signature = Get-AuthenticodeSignature -LiteralPath $exe
$inspection = [ordered]@{
    source_commit = $source
    pull_request_head_commit = $head
    merged_commit = 'cc3ee988284cb85614db824d0d02f510cf7d4046'
    candidate_artifact_id = 11564533643
    evidence_artifact_id = 11564164100
    archive_sha256 = $archiveHash
    executable_sha256 = $exeHash
    archive_entries = $entries
    original_validation = $validation.result
    authenticode_status = $signature.Status.ToString()
    pe_machine = ('0x{0:x4}' -f $machine)
    pe_dll_characteristics = ('0x{0:x4}' -f $characteristics)
    aslr = ($characteristics -band 0x0040) -ne 0
    high_entropy_aslr = ($characteristics -band 0x0020) -ne 0
    dep_nx = ($characteristics -band 0x0100) -ne 0
    control_flow_guard = ($characteristics -band 0x4000) -ne 0
    candidate_executed_in_this_review = $false
}
$inspection | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'candidate-inspection.json') -Encoding utf8
$inspection | ConvertTo-Json -Depth 8 | Write-Output
if (-not $inspection.aslr -or -not $inspection.dep_nx) {
    throw 'Baseline ASLR or DEP mitigation is missing.'
}

$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
$installation = & $vswhere -latest -products '*' -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Cannot locate PE inspection tools.' }
$dumpbin = Get-ChildItem -Path (Join-Path $installation 'VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe') |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
foreach ($mode in @('headers', 'dependents', 'imports')) {
    & $dumpbin.FullName ("/$mode") $exe |
        Out-File -LiteralPath (Join-Path $output ("pe-$mode.txt")) -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "PE $mode inspection failed." }
}
Get-Content -LiteralPath (Join-Path $output 'pe-dependents.txt')
# Embedded URLs are evidence for follow-up, not proof that a connection occurs.
$text = [Text.Encoding]::ASCII.GetString($bytes)
[Regex]::Matches($text, 'https?://[^\s"<>]+') |
    ForEach-Object Value | Sort-Object -Unique |
    Set-Content -LiteralPath (Join-Path $output 'embedded-urls.txt') -Encoding utf8
