<#
.SYNOPSIS
Builds an install-free inkpod ZIP for GitHub Releases.

.DESCRIPTION
Creates a flat, four-file archive after validating every input and the final
ZIP payload. ARM64 binaries use the requested `windows-arm` artifact suffix.
An existing valid archive is replaced only after the new archive is complete.
#>

[CmdletBinding()]
param(
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
    [string] $Version,

    [Parameter(Mandatory = $true)]
    [ValidateSet('x64', 'arm64')]
    [string] $Architecture,

    [Parameter(Mandatory = $true)]
    [string] $OutputPath
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

$portableArchitecture = if ($Architecture -eq 'arm64') { 'arm' } else { 'x64' }
$expectedFileName = "Inkpod-$Version-windows-$portableArchitecture.zip"
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
$actualFileName = [System.IO.Path]::GetFileName($resolvedOutput)
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
        throw "Required portable ZIP input does not exist: $($mapping.Source)"
    }
    if ((Get-Item -LiteralPath $mapping.Source).Length -eq 0) {
        throw "Required portable ZIP input is empty: $($mapping.Source)"
    }
    if ($resolvedOutput.Equals(
            $mapping.Source,
            [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Portable ZIP output would overwrite an input: $resolvedOutput"
    }
}

$outputDirectory = [System.IO.Path]::GetDirectoryName($resolvedOutput)
[System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$temporaryId = [System.Guid]::NewGuid().ToString('N')
$temporaryZip = Join-Path $outputDirectory ".$expectedFileName.$temporaryId.tmp.zip"
$backupZip = Join-Path $outputDirectory ".$expectedFileName.$temporaryId.backup"
$fixedTimestamp = [System.DateTimeOffset]::new(
    1980,
    1,
    1,
    0,
    0,
    0,
    [System.TimeSpan]::Zero)

try {
    $zipStream = [System.IO.File]::Open(
        $temporaryZip,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::None)
    $archive = $null
    try {
        $archive = [System.IO.Compression.ZipArchive]::new(
            $zipStream,
            [System.IO.Compression.ZipArchiveMode]::Create,
            $false)
        foreach ($mapping in $inputs) {
            $entry = $archive.CreateEntry(
                $mapping.EntryName,
                [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = $fixedTimestamp
            $sourceStream = [System.IO.File]::OpenRead($mapping.Source)
            $entryStream = $null
            try {
                $entryStream = $entry.Open()
                $sourceStream.CopyTo($entryStream)
            }
            finally {
                if ($null -ne $entryStream) {
                    $entryStream.Dispose()
                }
                $sourceStream.Dispose()
            }
        }
    }
    finally {
        if ($null -ne $archive) {
            $archive.Dispose()
        }
        else {
            $zipStream.Dispose()
        }
    }

    $archive = [System.IO.Compression.ZipFile]::OpenRead($temporaryZip)
    try {
        if ($archive.Entries.Count -ne $inputs.Count) {
            throw "Portable ZIP contains $($archive.Entries.Count) entries; expected $($inputs.Count)."
        }
        foreach ($mapping in $inputs) {
            $matchingEntries = @($archive.Entries | Where-Object {
                    $_.FullName -ceq $mapping.EntryName
                })
            if ($matchingEntries.Count -ne 1) {
                throw "Portable ZIP must contain exactly one '$($mapping.EntryName)' entry."
            }
            $entryStream = $matchingEntries[0].Open()
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
    }
    finally {
        $archive.Dispose()
    }

    if (Test-Path -LiteralPath $resolvedOutput -PathType Leaf) {
        [System.IO.File]::Replace($temporaryZip, $resolvedOutput, $backupZip, $true)
        [System.IO.File]::Delete($backupZip)
    }
    else {
        [System.IO.File]::Move($temporaryZip, $resolvedOutput)
    }

    Write-Host "Created portable ZIP: $resolvedOutput"
}
finally {
    foreach ($temporaryPath in @($temporaryZip, $backupZip)) {
        if (Test-Path -LiteralPath $temporaryPath -PathType Leaf) {
            [System.IO.File]::Delete($temporaryPath)
        }
    }
}
