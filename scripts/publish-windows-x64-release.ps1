<#
.SYNOPSIS
Publishes the existing Windows x64 executable as a GitHub prerelease.

.DESCRIPTION
Creates and pushes an annotated version tag, then creates a GitHub prerelease
whose asset is build/windows-x64-release/inkpod.exe. The tag and release title
are derived from the executable's embedded four-part product version.

.PARAMETER DryRun
Validates the version and executable, then prints the commands without creating
a tag, pushing to GitHub, or creating a release.

.EXAMPLE
.\scripts\publish-windows-x64-release.ps1

.EXAMPLE
.\scripts\publish-windows-x64-release.ps1 -DryRun
#>

[CmdletBinding()]
param(
    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path -Path $PSScriptRoot -ChildPath ".."))
$executablePath = Join-Path -Path $repositoryRoot `
    -ChildPath "build\windows-x64-release\inkpod.exe"

if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "Release executable does not exist: $executablePath"
}

$productVersion = (Get-Item -LiteralPath $executablePath).VersionInfo.ProductVersion
if ($productVersion -notmatch "^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$") {
    throw "Executable product version '$productVersion' is not a valid four-part version. Rebuild the release executable first."
}
$tag = "v$productVersion"
$tagMessage = "inkpod $tag"
$releaseTitle = "inkpod $productVersion"

$commands = @(
    [pscustomobject]@{
        Tool = "git.exe"
        Arguments = @("tag", "-a", $tag, "-m", $tagMessage)
    },
    [pscustomobject]@{
        Tool = "git.exe"
        Arguments = @("push", "origin", $tag)
    },
    [pscustomobject]@{
        Tool = "gh.exe"
        Arguments = @(
            "release",
            "create",
            $tag,
            $executablePath,
            "--verify-tag",
            "--generate-notes",
            "--title",
            $releaseTitle,
            "--prerelease")
    }
)

if ($DryRun) {
    Write-Host "Dry run: executable product version is $productVersion."
    foreach ($command in $commands) {
        Write-Host "> $($command.Tool) $($command.Arguments -join ' ')"
    }
    return
}

foreach ($tool in @("git.exe", "gh.exe")) {
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
    throw "The working tree is not clean. Commit or remove local changes before publishing."
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
} finally {
    Set-Location -LiteralPath $originalLocation
}
