# Windows candidate validation

The personal fork's Windows CI job keeps the normal `cargo xtask lint`
and `cargo xtask test` gates, then builds with the same static C runtime
flag as the portable release. It validates the built executable before
making a review candidate available. It does not publish a release.

Automatic Git inspection is disabled on Windows. Both checkout selection
and the Git subprocess boundary refuse it; the panel says **status
disabled**. Check repository changes, stashes and unpushed commits
separately before deciding to remove a checkout.

## Adjacent Git executable regression

Before the Windows lint and test gates, CI compiles the application's exact
Git module into a small probe and places an inert `git.exe` beside it. A
control call proves that the helper can create its marker. The probe then
selects a fixture checkout: Git state must be unavailable and the marker
must stay absent. The same fixture demonstrates execution with the original
vulnerable module. This runs on the hosted runner; it never launches the
candidate on the user's desktop.

## Evidence and candidate

Open the PR's **CI / Lint and test (Windows)** run. Three Actions artifacts
are used:

- `windows-validation-evidence`: build records, the locked dependency
  tree, PE imports when the SDK tool is available, Defender output, and
  `validation.json`. Evidence is kept even when validation fails.
- `windows-git-helper-regression`: records the adjacent-helper test,
  including its control and the checkout probe.
- `windows-candidate`: a ZIP and its SHA-256 sidecar. This exists only
  after the lint, tests, executable scan, startup check, and ZIP scan pass.

Artifacts are retained for 14 days. A missing scan engine, inactive
antivirus, failed signature update, signatures older than 48 hours,
nonzero scan exit, or changed executable fails validation. No executable
is uploaded after that failure. A skipped or unavailable scan is never
reported as a clean scan.

`BUILD-PROVENANCE.json` records the actual checked-out commit, PR head,
workflow revision, Actions run URL, runner image, toolchain, build command,
C runtime flag, lockfile hash, script hash, executable hash, and
Authenticode status. PR CI normally checks out GitHub's test merge commit;
the separate PR head field identifies the branch under review. These are
unsigned build records, not a cryptographic artifact attestation.

The smoke check executes only `disktree.exe --help`, after its scan,
and requires both a zero exit and the expected help text. The repository's
window-harness tests cover scan, selection, confirmation, and removal
against temporary trees; a CLI startup check does not verify a desktop GPU
or Explorer integration.

## Defender scan behavior

The script updates Defender's security intelligence and records its
engine, platform, signature version, signature timestamp, and protection
state. It calls `MpCmdRun.exe` separately for the executable and ZIP with
`-Scan -ScanType 3 -File <path> -DisableRemediation`.

Microsoft documents that this custom-scan option ignores file exclusions,
scans archives, and reports detections in command output without
remediating them. Without it, exit zero can also mean a threat was found
and remediated. The script changes no Defender settings and never restores
quarantined files. Real-time protection state is recorded separately; the
custom scan does not establish that real-time protection was enabled.

See [Microsoft's command-line reference](
https://learn.microsoft.com/en-us/defender-endpoint/command-line-arguments-microsoft-defender-antivirus).

A successful scan means that this Defender version completed its scan
without reporting a threat. It does not establish that upstream ZIPs are
clean or that all antivirus vendors agree. The fork's candidate is unsigned
unless its recorded Authenticode status says otherwise. No false-positive
submission or third-party binary upload is performed.

## Desktop validation still required

Use a VM or a disposable Windows account with Defender active. Verify the
ZIP hash against its sidecar, and the extracted executable against
`disktree.exe.sha256` and `BUILD-PROVENANCE.json`, before launching it.
Use the candidate from the same successful run as the evidence.

Create one isolated fixture, with an untouched sentinel beside the folder
being scanned:

```powershell
$fixture = Join-Path $env:TEMP ("disktree-review-" + [guid]::NewGuid())
$scanRoot = Join-Path $fixture 'scan'
New-Item -ItemType Directory -Path $scanRoot | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'outside-sentinel.txt') 'keep'
Set-Content -LiteralPath (Join-Path $scanRoot 'marked.txt') 'remove'
Set-Content -LiteralPath (Join-Path $scanRoot 'neighbour.txt') 'keep'
```

Open the candidate with `$scanRoot` as its path and check:

1. Scanning, navigation, and a cancelled confirmation leave both files and
   the outside sentinel intact.
2. Recycling `marked.txt` removes only that file and permits restoring it
   from the Recycle Bin. Record the restored contents.
3. Permanent deletion asks for confirmation and leaves `neighbour.txt`
   and the outside sentinel intact.
4. After marking a file, rename it and create a new file at the old path.
   Refresh must drop the mark; the new occupant must survive.
5. Select a directory containing `.git`. The panel must say **status
   disabled**, without treating the checkout as clean.
6. After a scan, rename the scanned folder and put a junction at its old
   path to another disposable folder. Removal from the old review must
   refuse, preserving sentinels in both folders.

Record the exact executable hash, Windows version, Defender state,
filesystem, and observed results. Windows 10 compatibility and a real
desktop GPU remain unverified until recorded; hosted Windows Server CI
does not substitute for them.

See [removal-safety.md](removal-safety.md) for the Recycle Bin concurrency
limit and the difference between object identity and immutable contents.
