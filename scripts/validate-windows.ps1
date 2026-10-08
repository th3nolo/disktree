# Inspect a built candidate; only a completed scan permits packaging.
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ExecutablePath,
    [Parameter(Mandatory)]
    [string] $OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Preserve native exit codes and logs, including a positive AV detection.
$PSNativeCommandUseErrorActionPreference = $false
$workRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Get-WorkspacePath([string] $name) {
    $resolved = [IO.Path]::GetFullPath($name, $workRoot)
    $prefix = $workRoot + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith(
        $prefix, [StringComparison]::OrdinalIgnoreCase
    )) {
        throw 'Validation paths must be inside this checkout.'
    }
    return $resolved
}

function Get-Sha256([string] $name) {
    $hash = Get-FileHash -LiteralPath $name -Algorithm SHA256
    return $hash.Hash.ToLowerInvariant()
}

function Invoke-LoggedNative(
    [string] $program, [string[]] $arguments, [string] $log
) {
    & $program @arguments 2>&1 | Out-File -LiteralPath $log -Encoding utf8
    if ($LASTEXITCODE -ne 0) {
        throw "$program exited $LASTEXITCODE; see $log"
    }
}

$built = Get-WorkspacePath $ExecutablePath
$output = Get-WorkspacePath $OutputDirectory
if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
    throw 'The built executable is missing.'
}
# Never overwrite evidence from an earlier invocation.
if (Test-Path -LiteralPath $output) {
    throw 'OutputDirectory already exists; choose a fresh directory.'
}
$evidence = Join-Path $output 'evidence'
$payload = Join-Path $output 'payload'
New-Item -ItemType Directory -Path $evidence, $payload | Out-Null
$report = [ordered]@{
    schema_version = 1
    result = 'incomplete'
    started_utc = [DateTime]::UtcNow.ToString('o')
    finished_utc = $null
    source_commit = $null
    executable_sha256 = $null
    archive_sha256 = $null
    defender = $null
    executable_scan_exit_code = $null
    archive_scan_exit_code = $null
    help_exit_code = $null
    imports = 'unavailable'
    error = $null
}

try {
    $commit = (& git -C $workRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-f]{40}$') {
        throw 'Cannot identify the source checkout.'
    }
    $report.source_commit = $commit
    $exe = Join-Path $payload 'disktree.exe'
    Copy-Item -LiteralPath $built -Destination $exe
    $exeHash = Get-Sha256 $exe
    $report.executable_sha256 = $exeHash
    if ((Get-Sha256 $built) -ne $exeHash) {
        throw 'The candidate differs from the built executable.'
    }

    Invoke-LoggedNative rustc @('--version', '--verbose') (
        Join-Path $evidence 'rustc.txt'
    )
    Invoke-LoggedNative cargo @('--version', '--verbose') (
        Join-Path $evidence 'cargo.txt'
    )
    Invoke-LoggedNative cargo @(
        'tree', '--locked', '--workspace', '--all-features'
    ) (Join-Path $evidence 'dependencies.txt')
    $signature = Get-AuthenticodeSignature -LiteralPath $exe
    $provenance = [ordered]@{
        schema_version = 1
        repository = $env:GITHUB_REPOSITORY
        source_commit = $commit
        pull_request_head_commit = $env:PR_HEAD_SHA
        workflow_revision = $env:WORKFLOW_REVISION
        workflow = $env:GITHUB_WORKFLOW
        workflow_run_url = (
            "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY" +
            "/actions/runs/$env:GITHUB_RUN_ID"
        )
        workflow_run_attempt = $env:GITHUB_RUN_ATTEMPT
        runner_image = $env:ImageOS
        runner_image_version = $env:ImageVersion
        build_command = 'cargo build --release --locked -p disktree-app'
        rustflags = $env:RUSTFLAGS
        rustc = Get-Content -LiteralPath (
            Join-Path $evidence 'rustc.txt'
        ) -Raw
        cargo = Get-Content -LiteralPath (
            Join-Path $evidence 'cargo.txt'
        ) -Raw
        cargo_lock_sha256 = Get-Sha256 (Join-Path $workRoot 'Cargo.lock')
        validation_script_sha256 = Get-Sha256 $PSCommandPath
        executable_sha256 = $exeHash
        authenticode_status = $signature.Status.ToString()
        cryptographic_attestation = $false
    }
    $provenance | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (
        Join-Path $payload 'BUILD-PROVENANCE.json'
    ) -Encoding utf8
    Copy-Item -LiteralPath (
        Join-Path $payload 'BUILD-PROVENANCE.json'
    ) -Destination $evidence

    # Inspect imports without executing the candidate.
    $programFilesX86 = [Environment]::GetEnvironmentVariable(
        'ProgramFiles(x86)'
    )
    $vswhere = Join-Path $programFilesX86 (
        'Microsoft Visual Studio/Installer/vswhere.exe'
    )
    if (Test-Path -LiteralPath $vswhere) {
        $installation = & $vswhere -latest -products '*' -requires (
            'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
        ) -property installationPath
        if ($LASTEXITCODE -eq 0 -and $installation) {
            $dumpbin = Get-ChildItem -Path (
                Join-Path $installation (
                    'VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe'
                )
            ) -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($dumpbin) {
                Invoke-LoggedNative $dumpbin.FullName @('/imports', $exe) (
                    Join-Path $evidence 'imports.txt'
                )
                $report.imports = 'recorded'
            }
        }
    }

    # No exclusions, protection changes, or quarantine overrides.
    Update-MpSignature -ErrorAction Stop
    $status = Get-MpComputerStatus -ErrorAction Stop
    $report.defender = $status | Select-Object AMServiceEnabled,
        AntivirusEnabled, RealTimeProtectionEnabled, AMProductVersion,
        AMEngineVersion, AntivirusSignatureVersion,
        AntivirusSignatureLastUpdated
    if (-not $status.AMServiceEnabled -or -not $status.AntivirusEnabled) {
        throw 'Microsoft Defender Antivirus is unavailable or inactive.'
    }
    if ($status.AntivirusSignatureLastUpdated.ToUniversalTime() -lt (
        [DateTime]::UtcNow.AddDays(-2)
    )) {
        throw 'Defender security intelligence is older than 48 hours.'
    }
    $platform = Join-Path $env:ProgramData (
        'Microsoft/Windows Defender/Platform'
    )
    $scanner = Get-ChildItem -Path (
        Join-Path $platform '*/MpCmdRun.exe'
    ) -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    $scannerPath = if ($scanner) {
        $scanner.FullName
    } else {
        Join-Path $env:ProgramFiles 'Windows Defender/MpCmdRun.exe'
    }
    if (-not (Test-Path -LiteralPath $scannerPath -PathType Leaf)) {
        throw 'MpCmdRun.exe is unavailable.'
    }
    # For custom scans this ignores file exclusions and scans archives.
    # It prevents remediation from turning "detected" into exit code zero.
    & $scannerPath -Scan -ScanType 3 -File $exe -DisableRemediation 2>&1 |
        Out-File -LiteralPath (
            Join-Path $evidence 'defender-executable.txt'
        ) -Encoding utf8
    $report.executable_scan_exit_code = $LASTEXITCODE
    if ($LASTEXITCODE -ne 0) {
        throw 'The executable scan detected a threat or failed.'
    }

    # Run the scanned executable with a non-GUI help request.
    $stdout = Join-Path $evidence 'help-stdout.txt'
    $stderr = Join-Path $evidence 'help-stderr.txt'
    $start = @{
        FilePath = $exe
        ArgumentList = '--help'
        PassThru = $true
        WindowStyle = 'Hidden'
        RedirectStandardOutput = $stdout
        RedirectStandardError = $stderr
    }
    $process = Start-Process @start
    if (-not $process.WaitForExit(30000)) {
        $process.Kill()
        $process.WaitForExit()
        throw 'The executable help smoke check timed out.'
    }
    $process.WaitForExit()
    $report.help_exit_code = $process.ExitCode
    if ($process.ExitCode -ne 0 -or (
        Get-Content -LiteralPath $stdout -Raw
    ) -notmatch 'usage: disktree') {
        throw 'The executable did not return its expected help text.'
    }

    foreach ($name in @('README.md', 'LICENSE')) {
        $source = Join-Path $workRoot $name
        Copy-Item -LiteralPath $source -Destination $payload
    }
    Copy-Item -LiteralPath (
        Join-Path $workRoot 'docs/removal-safety.md'
    ) -Destination $payload
    Copy-Item -LiteralPath (
        Join-Path $workRoot 'docs/windows-validation.md'
    ) -Destination $payload
    $exeHash + '  disktree.exe' | Set-Content -LiteralPath (
        Join-Path $payload 'disktree.exe.sha256'
    ) -Encoding utf8NoBOM
    $archive = Join-Path $output (
        'disktree-candidate-' + $commit.Substring(0, 12) + '-windows.zip'
    )
    $archiveInputs = Join-Path $payload '*'
    Compress-Archive -Path $archiveInputs -DestinationPath $archive
    & $scannerPath -Scan -ScanType 3 -File $archive -DisableRemediation 2>&1 |
        Out-File -LiteralPath (
            Join-Path $evidence 'defender-archive.txt'
        ) -Encoding utf8
    $report.archive_scan_exit_code = $LASTEXITCODE
    if ($LASTEXITCODE -ne 0) {
        throw 'The archive scan detected a threat or failed.'
    }
    if ((Get-Sha256 $exe) -ne $exeHash -or (
        Get-Sha256 $built
    ) -ne $exeHash) {
        throw 'The executable changed during validation.'
    }
    $archiveHash = Get-Sha256 $archive
    $report.archive_sha256 = $archiveHash
    $archiveHash + '  ' + [IO.Path]::GetFileName($archive) |
        Set-Content -LiteralPath ($archive + '.sha256') -Encoding utf8NoBOM
    $report.result = 'passed'
} catch {
    $report.error = $_.Exception.Message
    throw
} finally {
    $report.finished_utc = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (
        Join-Path $evidence 'validation.json'
    ) -Encoding utf8
    if ($env:GITHUB_STEP_SUMMARY) {
        @(
            '### Windows candidate validation'
            ''
            "| Check | Result |"
            "| --- | --- |"
            "| Validation | $($report.result) |"
            "| Source | $($report.source_commit) |"
            "| EXE scan exit | $($report.executable_scan_exit_code) |"
            "| ZIP scan exit | $($report.archive_scan_exit_code) |"
            "| Help exit | $($report.help_exit_code) |"
            ''
            'This is a scan result, not a malware-free certification.'
            'Desktop GPU, Explorer, and Windows 10 checks remain manual.'
        ) | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY
    }
}
