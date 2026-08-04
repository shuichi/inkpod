<#
.SYNOPSIS
Publishes the existing Windows x64 and ARM64 portable ZIPs as a GitHub prerelease.

.DESCRIPTION
Creates and pushes an annotated semantic-version tag, then creates a GitHub
prerelease with both portable ZIP assets. The first three components of the
embedded executable product versions identify the release; the fourth build
component remains specific to the EXE and MSIX artifacts.

.PARAMETER DryRun
Validates both executables and ZIP assets, then prints the commands without
creating a tag, pushing to GitHub, or creating a release.

.EXAMPLE
.\scripts\publish-windows-release.ps1

.EXAMPLE
.\scripts\publish-windows-release.ps1 -DryRun
#>

[CmdletBinding()]
param(
    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-SemanticVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Executable
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Release executable does not exist: $Executable"
    }
    $productVersion = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
    $match = [System.Text.RegularExpressions.Regex]::Match(
        $productVersion,
        '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')
    if (-not $match.Success) {
        throw "Executable product version '$productVersion' is not a valid four-part version: $Executable"
    }
    return "$($match.Groups[1].Value).$($match.Groups[2].Value).$($match.Groups[3].Value)"
}

$repositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path -Path $PSScriptRoot -ChildPath '..'))
$x64Executable = Join-Path $repositoryRoot 'build\windows-x64-release\inkpod.exe'
$armExecutable = Join-Path $repositoryRoot 'build\windows-arm-release\inkpod.exe'
$x64Version = Get-SemanticVersion -Executable $x64Executable
$armVersion = Get-SemanticVersion -Executable $armExecutable
if ($x64Version -cne $armVersion) {
    throw "Windows release architectures have different semantic versions: x64=$x64Version, ARM64=$armVersion"
}

$version = $x64Version
$x64Archive = Join-Path $repositoryRoot `
    "build\windows-x64-release\package\Inkpod-$version-windows-x64.zip"
$armArchive = Join-Path $repositoryRoot `
    "build\windows-arm-release\package\Inkpod-$version-windows-arm.zip"
foreach ($archive in @($x64Archive, $armArchive)) {
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        throw "Portable release archive does not exist: $archive"
    }
}

$tag = "v$version"
$tagMessage = "inkpod $tag"
$releaseTitle = "inkpod $version"
$commands = @(
    [pscustomobject]@{
        Tool = 'git.exe'
        Arguments = @('tag', '-a', $tag, '-m', $tagMessage)
    },
    [pscustomobject]@{
        Tool = 'git.exe'
        Arguments = @('push', 'origin', $tag)
    },
    [pscustomobject]@{
        Tool = 'gh.exe'
        Arguments = @(
            'release',
            'create',
            $tag,
            $x64Archive,
            $armArchive,
            '--verify-tag',
            '--generate-notes',
            '--title',
            $releaseTitle,
            '--prerelease')
    }
)

if ($DryRun) {
    Write-Host "Dry run: Windows portable release version is $version."
    foreach ($command in $commands) {
        Write-Host "> $($command.Tool) $($command.Arguments -join ' ')"
    }
    return
}

foreach ($tool in @('git.exe', 'gh.exe')) {
    if ($null -eq (Get-Command -Name $tool -CommandType Application `
            -ErrorAction SilentlyContinue)) {
        throw "Required command was not found: $tool"
    }
}

$workingTreeStatus = & git.exe -C $repositoryRoot status --porcelain
if ($LASTEXITCODE -ne 0) {
    throw "git.exe status failed with exit code $LASTEXITCODE."
}
if ($workingTreeStatus) {
    throw 'The working tree is not clean. Commit or remove local changes before publishing.'
}

$originalLocation = Get-Location
try {
    Set-Location -LiteralPath $repositoryRoot
    foreach ($command in $commands) {
        Write-Host "> $($command.Tool) $($command.Arguments -join ' ')"
        $tool = $command.Tool
        $arguments = @($command.Arguments)
        & $tool @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$($command.Tool) failed with exit code $LASTEXITCODE."
        }
    }
}
finally {
    Set-Location -LiteralPath $originalLocation
}
