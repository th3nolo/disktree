# Inspect the image without loading or executing its code.
# Offsets follow Microsoft's PE32+ load-configuration specification.
function Get-WindowsPeMitigations {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $LiteralPath)

    Add-Type -AssemblyName System.Reflection.Metadata
    $bytes = [IO.File]::ReadAllBytes($LiteralPath)
    $stream = [IO.MemoryStream]::new($bytes, $false)
    $reader = [System.Reflection.PortableExecutable.PEReader]::new($stream)
    try {
        $headers = $reader.PEHeaders
        $pe = $headers.PEHeader
        if ($null -eq $pe -or [int] $pe.Magic -ne 0x20b -or (
            [int] $headers.CoffHeader.Machine -notin @(0x8664, 0xaa64)
        )) {
            throw 'Expected an x64 or ARM64 PE32+ executable.'
        }
        $dll = [int] $pe.DllCharacteristics
        if (($dll -band 0x4000) -eq 0) {
            throw 'Control Flow Guard is absent from the executable.'
        }
        # Keep ASLR, high-entropy ASLR and DEP alongside CFG.
        if (($dll -band 0x0160) -ne 0x0160) {
            throw 'ASLR, high-entropy ASLR or DEP is absent.'
        }
        $directory = $pe.LoadConfigTableDirectory
        $loadOffset = 0
        if ($directory.Size -lt 148 -or -not (
            $headers.TryGetDirectoryOffset($directory, [ref] $loadOffset)
        )) {
            throw 'The CFG load configuration is missing or too short.'
        }
        [byte[]] $config = $reader.GetSectionData(
            $directory.RelativeVirtualAddress
        ).GetContent(0, 148)
        if ([BitConverter]::ToUInt32($config, 0) -lt 148) {
            throw 'The CFG load configuration has an invalid size.'
        }
        $flags = [BitConverter]::ToUInt32($config, 144)
        $table = [BitConverter]::ToUInt64($config, 128)
        $count = [BitConverter]::ToUInt64($config, 136)
        # CF_INSTRUMENTED and CF_FUNCTION_TABLE_PRESENT must both be set.
        if (($flags -band 0x0500) -ne 0x0500 -or $count -eq 0) {
            throw 'CFG instrumentation or its function table is absent.'
        }
        if ($table -lt $pe.ImageBase -or (
            $table - $pe.ImageBase -ge $pe.SizeOfImage
        )) {
            throw 'The CFG function table is outside the image.'
        }
        # The top nibble gives extra bytes per entry beyond its 4-byte RVA.
        $stride = 4 + (($flags -shr 28) -band 0xf)
        if ($count -gt [math]::Floor($bytes.LongLength / $stride)) {
            throw 'The CFG function table exceeds the file.'
        }
        $null = $reader.GetSectionData(
            [int] ($table - $pe.ImageBase)
        ).GetContent(0, [int] ($count * $stride))
        return [pscustomobject]@{
            architecture = $headers.CoffHeader.Machine.ToString()
            dll_characteristics = $dll
            cfg = $true
            aslr = $true
            high_entropy_aslr = $true
            dep = $true
            guard_flags = $flags
            guard_function_count = $count
            dll_characteristics_offset = $headers.PEHeaderStartOffset + 70
            load_config_offset = $loadOffset
        }
    } finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}
