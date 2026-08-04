param(
    [Parameter(Mandatory = $true)]
    [string] $PackagePath,

    [Parameter(Mandatory = $true)]
    [string] $Executable,

    [Parameter(Mandatory = $true)]
    [string] $Readme,

    [Parameter(Mandatory = $true)]
    [string] $License,

    [Parameter(Mandatory = $true)]
    [string] $ThirdPartyNotices,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string] $ExpectedVersion,

    [Parameter(Mandatory = $true)]
    [ValidateSet('x64', 'arm64')]
    [string] $ExpectedArchitecture,

    [switch] $RunAbiSmoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Get-Sha256Hex {
    param(
        [Parameter(Mandatory = $true)]
        [System.IO.Stream] $Stream
    )

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString(
                $sha256.ComputeHash($Stream))).Replace('-', '')
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-PeMachine {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    $stream = [System.IO.File]::OpenRead($Path)
    $reader = $null
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "Portable executable does not have an MZ header: $Path"
        }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        if ($peOffset -gt ($stream.Length - 6)) {
            throw "Portable executable has an invalid PE offset: $peOffset"
        }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "Portable executable does not have a PE header: $Path"
        }
        return $reader.ReadUInt16()
    }
    finally {
        if ($null -ne $reader) {
            $reader.Dispose()
        }
        else {
            $stream.Dispose()
        }
    }
}

function Remove-TemporaryDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    for ($attempt = 1; $attempt -le 20; $attempt++) {
        if (-not (Test-Path -LiteralPath $Path)) {
            return
        }
        try {
            Remove-Item -LiteralPath $Path -Recurse -Force
            return
        }
        catch {
            if ($attempt -eq 20) {
                throw
            }
            Start-Sleep -Milliseconds 100
        }
    }
}

$resolvedPackage = [System.IO.Path]::GetFullPath($PackagePath)
if (-not (Test-Path -LiteralPath $resolvedPackage -PathType Leaf)) {
    throw "Portable ZIP does not exist: $resolvedPackage"
}

$portableArchitecture = if ($ExpectedArchitecture -eq 'arm64') { 'arm' } else { 'x64' }
$expectedFileName = "Inkpod-$ExpectedVersion-windows-$portableArchitecture.zip"
$actualFileName = [System.IO.Path]::GetFileName($resolvedPackage)
if (-not $actualFileName.Equals($expectedFileName, [System.StringComparison]::Ordinal)) {
    throw "Portable ZIP must be named '$expectedFileName', not '$actualFileName'."
}

$inputs = @(
    [pscustomobject]@{ Source = $Executable; EntryName = 'inkpod.exe' },
    [pscustomobject]@{ Source = $Readme; EntryName = 'README.txt' },
    [pscustomobject]@{ Source = $License; EntryName = 'LICENSE.txt' },
    [pscustomobject]@{ Source = $ThirdPartyNotices; EntryName = 'ThirdPartyNotices.txt' }
)
foreach ($mapping in $inputs) {
    $mapping.Source = [System.IO.Path]::GetFullPath($mapping.Source)
    if (-not (Test-Path -LiteralPath $mapping.Source -PathType Leaf)) {
        throw "Portable ZIP comparison input does not exist: $($mapping.Source)"
    }
}

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$temporaryPrefix = $temporaryRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) +
    [System.IO.Path]::DirectorySeparatorChar + 'inkpod-portable-payload-'
$temporaryDirectory = $temporaryPrefix + [System.Guid]::NewGuid().ToString('N')
$extractedExecutable = Join-Path $temporaryDirectory 'inkpod.exe'

try {
    [System.IO.Directory]::CreateDirectory($temporaryDirectory) | Out-Null
    $archive = [System.IO.Compression.ZipFile]::OpenRead($resolvedPackage)
    try {
        if ($archive.Entries.Count -ne $inputs.Count) {
            throw "Portable ZIP contains $($archive.Entries.Count) entries; expected $($inputs.Count)."
        }
        foreach ($mapping in $inputs) {
            $matchingEntries = @($archive.Entries | Where-Object {
                    $_.FullName -ceq $mapping.EntryName
                })
            if ($matchingEntries.Count -ne 1) {
                throw "Portable ZIP must contain exactly one '$($mapping.EntryName)' root entry."
            }
            $entry = $matchingEntries[0]
            if ($entry.Name -cne $entry.FullName) {
                throw "Portable ZIP entry is nested below the root: $($entry.FullName)"
            }

            $entryStream = $entry.Open()
            try {
                $entryHash = Get-Sha256Hex -Stream $entryStream
            }
            finally {
                $entryStream.Dispose()
            }
            $sourceStream = [System.IO.File]::OpenRead($mapping.Source)
            try {
                $sourceHash = Get-Sha256Hex -Stream $sourceStream
            }
            finally {
                $sourceStream.Dispose()
            }
            if ($entryHash -cne $sourceHash) {
                throw "Portable ZIP entry '$($mapping.EntryName)' differs from its source."
            }
        }

        $executableEntry = @($archive.Entries | Where-Object {
                $_.FullName -ceq 'inkpod.exe'
            })[0]
        $entryStream = $executableEntry.Open()
        $outputStream = $null
        try {
            $outputStream = [System.IO.File]::Open(
                $extractedExecutable,
                [System.IO.FileMode]::CreateNew,
                [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None)
            $entryStream.CopyTo($outputStream)
        }
        finally {
            if ($null -ne $outputStream) {
                $outputStream.Dispose()
            }
            $entryStream.Dispose()
        }
    }
    finally {
        $archive.Dispose()
    }

    $expectedMachine = if ($ExpectedArchitecture -eq 'arm64') { 0xAA64 } else { 0x8664 }
    $actualMachine = Get-PeMachine -Path $extractedExecutable
    if ($actualMachine -ne $expectedMachine) {
        throw ('Portable executable machine 0x{0:X4} does not match expected 0x{1:X4}.' -f
            $actualMachine, $expectedMachine)
    }

    $productVersion = (Get-Item -LiteralPath $extractedExecutable).VersionInfo.ProductVersion
    $versionPattern = '^' + [System.Text.RegularExpressions.Regex]::Escape($ExpectedVersion) +
        '\.(0|[1-9][0-9]*)$'
    if ($productVersion -notmatch $versionPattern) {
        throw "Portable executable version '$productVersion' does not match '$ExpectedVersion.<build>'."
    }

    if ($RunAbiSmoke) {
        if ($ExpectedArchitecture -ne 'x64') {
            throw 'The portable ABI smoke is enabled only for the native x64 test target.'
        }
        $process = $null
        $exitCode = $null
        try {
            $process = Start-Process `
                -FilePath $extractedExecutable `
                -ArgumentList '--abi-smoke-test' `
                -Wait `
                -PassThru `
                -WindowStyle Hidden
            $exitCode = $process.ExitCode
        }
        finally {
            if ($null -ne $process) {
                $process.Dispose()
            }
        }
        if ($exitCode -ne 0) {
            throw "Portable inkpod ABI smoke failed with exit code $exitCode."
        }
    }
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        $resolvedTemporary = [System.IO.Path]::GetFullPath($temporaryDirectory)
        if (-not $resolvedTemporary.StartsWith(
                $temporaryPrefix,
                [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove unexpected temporary path: $resolvedTemporary"
        }
        Remove-TemporaryDirectory -Path $resolvedTemporary
    }
}
