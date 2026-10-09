# Compile inert fixtures and inspect bytes; never execute either fixture.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'windows-pe.ps1')

$peTestRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'disktree-pe-' + [guid]::NewGuid().ToString('N')
)
New-Item -ItemType Directory -Path $peTestRoot | Out-Null
$source = Join-Path $peTestRoot 'indirect.rs'
@'
fn add(value: usize) -> usize { value + 1 }
fn sub(value: usize) -> usize { value.saturating_sub(1) }
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let call: fn(usize) -> usize =
        if args.len() > 1 { add } else { sub };
    println!("{}", call(args.len()));
}
'@ | Set-Content -LiteralPath $source -Encoding utf8
$plain = Join-Path $peTestRoot 'without-cfg.exe'
$guarded = Join-Path $peTestRoot 'with-cfg.exe'
foreach ($build in @(
    @{ Flag = 'no'; Path = $plain },
    @{ Flag = 'yes'; Path = $guarded }
)) {
    & rustc --edition=2024 -C opt-level=0 -C target-feature=+crt-static (
        '-C'
    ) ("control-flow-guard=" + $build.Flag) $source -o $build.Path
    if ($LASTEXITCODE -ne 0) {
        throw "Fixture compilation failed: $($build.Flag)"
    }
}

function Assert-Refused([string] $name, [string] $path, [string] $reason) {
    $failure = $null
    try { $null = Get-WindowsPeMitigations -LiteralPath $path }
    catch { $failure = $_.Exception.Message }
    if ($null -eq $failure -or (
        $reason -and $failure -notmatch $reason
    )) {
        throw "$name was not refused for the expected reason: $failure"
    }
    Write-Host "PASS: $name refused ($failure)"
}

Assert-Refused 'compiler CFG disabled' $plain 'Control Flow Guard'
$info = Get-WindowsPeMitigations -LiteralPath $guarded
Write-Host 'PASS: compiler CFG enabled accepted'
[byte[]] $original = [IO.File]::ReadAllBytes($guarded)
foreach ($mutation in @(
    @{ Name = 'missing CFG flag'; Offset = $info.dll_characteristics_offset;
       Bytes = [BitConverter]::GetBytes(
           [uint16] ($info.dll_characteristics -band 0xbfff)
       ); Reason = 'Control Flow Guard' },
    @{ Name = 'missing instrumentation'; Offset = $info.load_config_offset + 144;
       Bytes = [BitConverter]::GetBytes([uint32] 0);
       Reason = 'instrumentation' },
    @{ Name = 'empty function table'; Offset = $info.load_config_offset + 136;
       Bytes = [BitConverter]::GetBytes([uint64] 0);
       Reason = 'function table' },
    @{ Name = 'table outside image'; Offset = $info.load_config_offset + 128;
       Bytes = [BitConverter]::GetBytes([uint64] 0);
       Reason = 'outside the image' }
)) {
    [byte[]] $copy = $original.Clone()
    [Array]::Copy($mutation.Bytes, 0, $copy, $mutation.Offset,
                  $mutation.Bytes.Length)
    $path = Join-Path $peTestRoot ($mutation.Name.Replace(' ', '-') + '.exe')
    [IO.File]::WriteAllBytes($path, $copy)
    Assert-Refused $mutation.Name $path $mutation.Reason
}
$truncated = Join-Path $peTestRoot 'truncated.exe'
[IO.File]::WriteAllBytes($truncated, [byte[]] @(0x4d, 0x5a))
Assert-Refused 'truncated image' $truncated ''
Write-Host 'PE mitigation regressions: 7 passed; no executable launched.'
