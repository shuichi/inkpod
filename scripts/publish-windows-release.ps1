<#
.SYNOPSIS
Builds and publishes a versioned Windows GitHub prerelease.

.DESCRIPTION
This is the release orchestrator for the x64 and ARM64 portable ZIPs. In
publish mode it:

1. verifies a clean, synchronized release branch and GitHub CLI login;
2. updates every application-version source and commits/pushes that change;
3. clean-builds both Windows Release presets and runs native-architecture
   CTest plus architecture-independent package validation;
4. creates and pushes an annotated tag, then creates a GitHub prerelease with
   both portable ZIPs;
5. reads the uploaded asset URLs from GitHub, updates html/index.html, and
   commits/pushes the download-link change.

The version-bump and download-link commits are intentionally separate so the
tag names the exact source used for the uploaded binaries. A failure after the
version-bump commit or tag push can be retried with the same version while the
working tree remains clean.

.PARAMETER Version
Three-part semantic application version to publish.

.PARAMETER Branch
Release branch that must be checked out and synchronized with origin.

.PARAMETER DryRun
Validates all local replacement contracts and prints the planned commands. It
does not edit files, build, commit, push, tag, or contact GitHub.

.PARAMETER Publish
Explicitly enables local edits, builds, commits, pushes, and GitHub release
creation. Specify exactly one of DryRun or Publish.

.EXAMPLE
.\scripts\publish-windows-release.ps1 -Version 0.2.1 -DryRun

.EXAMPLE
.\scripts\publish-windows-release.ps1 -Version 0.2.1 -Publish
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string] $Version,

    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._/-]*$')]
    [string] $Branch = 'main',

    [switch] $DryRun,
    [switch] $Publish
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($DryRun -eq $Publish) {
    throw 'Specify exactly one of -DryRun or -Publish.'
}

$repositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path -Path $PSScriptRoot -ChildPath '..'))
$versionFiles = @(
    'CMakeLists.txt',
    'Cargo.toml',
    'Cargo.lock',
    'rust/inkpod-format/fuzz/Cargo.lock',
    'apps/windows/package/Package.appxmanifest',
    'apps/windows/app/app.manifest',
    'README.md',
    'docs/windows-packaging.md'
)
$downloadPageRelativePath = 'html/index.html'
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Get-RepositoryPath {
    param([Parameter(Mandatory = $true)][string] $RelativePath)

    return Join-Path -Path $repositoryRoot -ChildPath (
        $RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar))
}

function Get-CommandLine {
    param(
        [Parameter(Mandatory = $true)][string] $Tool,
        [Parameter(Mandatory = $true)][string[]] $Arguments
    )

    $tokens = foreach ($argument in $Arguments) {
        if ($argument -match '[\s"]') {
            '"' + $argument.Replace('"', '\"') + '"'
        }
        else {
            $argument
        }
    }
    return "$Tool $($tokens -join ' ')"
}

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory = $true)][string] $Tool,
        [Parameter(Mandatory = $true)][string[]] $Arguments,
        [switch] $CaptureOutput
    )

    Write-Host "> $(Get-CommandLine -Tool $Tool -Arguments $Arguments)"
    if ($CaptureOutput) {
        $output = @(& $Tool @Arguments)
        if ($LASTEXITCODE -ne 0) {
            throw "$Tool failed with exit code $LASTEXITCODE."
        }
        return ($output -join "`n").Trim()
    }

    & $Tool @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Tool failed with exit code $LASTEXITCODE."
    }
}

function Invoke-OptionalCommand {
    param(
        [Parameter(Mandatory = $true)][string] $Tool,
        [Parameter(Mandatory = $true)][string[]] $Arguments
    )

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell turns native stderr into an ErrorRecord. Probe
        # commands intentionally use nonzero exit codes for "not found", so
        # keep that expected result from becoming a terminating error while
        # the rest of this release script remains fail-fast.
        $ErrorActionPreference = 'SilentlyContinue'
        $output = @(& $Tool @Arguments 2>$null)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = ($output -join "`n").Trim()
    }
}

function Assert-CommandAvailable {
    param([Parameter(Mandatory = $true)][string] $Name)

    if ($null -eq (Get-Command -Name $Name -CommandType Application `
            -ErrorAction SilentlyContinue)) {
        throw "Required command was not found: $Name"
    }
}

function Replace-RegexExactly {
    param(
        [Parameter(Mandatory = $true)][string] $Text,
        [Parameter(Mandatory = $true)][string] $Pattern,
        [Parameter(Mandatory = $true)][scriptblock] $Replacement,
        [Parameter(Mandatory = $true)][int] $ExpectedCount,
        [Parameter(Mandatory = $true)][string] $Description
    )

    $regex = [System.Text.RegularExpressions.Regex]::new($Pattern)
    $matches = $regex.Matches($Text)
    if ($matches.Count -ne $ExpectedCount) {
        throw "Expected $ExpectedCount $Description match(es), found $($matches.Count)."
    }
    return $regex.Replace(
        $Text,
        [System.Text.RegularExpressions.MatchEvaluator] {
            param($match)
            return & $Replacement $match
        })
}

function Set-TextFileAtomically {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $Text
    )

    $temporaryPath = "$Path.$PID.tmp"
    $backupPath = "$Path.$PID.backup"
    try {
        [System.IO.File]::WriteAllText($temporaryPath, $Text, $utf8NoBom)
        [System.IO.File]::Replace($temporaryPath, $Path, $backupPath, $true)
        [System.IO.File]::Delete($backupPath)
    }
    finally {
        foreach ($temporaryFile in @($temporaryPath, $backupPath)) {
            if (Test-Path -LiteralPath $temporaryFile -PathType Leaf) {
                [System.IO.File]::Delete($temporaryFile)
            }
        }
    }
}

function Get-CurrentProjectVersion {
    $cmakePath = Get-RepositoryPath -RelativePath 'CMakeLists.txt'
    $cmake = [System.IO.File]::ReadAllText($cmakePath)
    $match = [regex]::Match(
        $cmake,
        '(?m)^project\(inkpod VERSION (?<version>[0-9]+\.[0-9]+\.[0-9]+) LANGUAGES C CXX\)\r?$')
    if (-not $match.Success) {
        throw 'CMakeLists.txt does not contain the canonical inkpod project version.'
    }
    return $match.Groups['version'].Value
}

function Update-LockPackageVersions {
    param(
        [Parameter(Mandatory = $true)][string] $Text,
        [Parameter(Mandatory = $true)][string[]] $PackageNames,
        [Parameter(Mandatory = $true)][string] $CurrentVersion,
        [Parameter(Mandatory = $true)][string] $NewVersion,
        [Parameter(Mandatory = $true)][string] $RelativePath
    )

    $updated = $Text
    foreach ($packageName in $PackageNames) {
        $escapedName = [regex]::Escape($packageName)
        $escapedVersion = [regex]::Escape($CurrentVersion)
        $pattern = '(?m)(^name = "' + $escapedName + '"\r?\nversion = ")' +
            $escapedVersion + '(")\r?$'
        $updated = Replace-RegexExactly `
            -Text $updated `
            -Pattern $pattern `
            -ExpectedCount 1 `
            -Description "$packageName version in $RelativePath" `
            -Replacement {
                param($match)
                return $match.Groups[1].Value + $NewVersion + $match.Groups[2].Value
            }
    }
    return $updated
}

function Get-VersionFileUpdates {
    param(
        [Parameter(Mandatory = $true)][string] $CurrentVersion,
        [Parameter(Mandatory = $true)][string] $NewVersion
    )

    $updates = [ordered]@{}

    $relativePath = 'CMakeLists.txt'
    $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
    $updates[$relativePath] = Replace-RegexExactly `
        -Text $text `
        -Pattern ('(?m)^(project\(inkpod VERSION )' +
            [regex]::Escape($CurrentVersion) + '( LANGUAGES C CXX\))\r?$') `
        -ExpectedCount 1 `
        -Description 'CMake project version' `
        -Replacement {
            param($match)
            return $match.Groups[1].Value + $NewVersion + $match.Groups[2].Value
        }

    $relativePath = 'Cargo.toml'
    $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
    $updates[$relativePath] = Replace-RegexExactly `
        -Text $text `
        -Pattern ('(?m)^(version = ")' + [regex]::Escape($CurrentVersion) + '(")\r?$') `
        -ExpectedCount 1 `
        -Description 'Cargo workspace package version' `
        -Replacement {
            param($match)
            return $match.Groups[1].Value + $NewVersion + $match.Groups[2].Value
        }

    $relativePath = 'Cargo.lock'
    $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
    $updates[$relativePath] = Update-LockPackageVersions `
        -Text $text `
        -PackageNames @(
            'inkpod-core',
            'inkpod-ffi',
            'inkpod-format',
            'inkpod-image',
            'inkpod-io') `
        -CurrentVersion $CurrentVersion `
        -NewVersion $NewVersion `
        -RelativePath $relativePath

    $relativePath = 'rust/inkpod-format/fuzz/Cargo.lock'
    $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
    $updates[$relativePath] = Update-LockPackageVersions `
        -Text $text `
        -PackageNames @('inkpod-core', 'inkpod-format', 'inkpod-image', 'inkpod-io') `
        -CurrentVersion $CurrentVersion `
        -NewVersion $NewVersion `
        -RelativePath $relativePath

    foreach ($relativePath in @(
            'apps/windows/package/Package.appxmanifest',
            'apps/windows/app/app.manifest')) {
        $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
        $updates[$relativePath] = Replace-RegexExactly `
            -Text $text `
            -Pattern ('(?im)(version=")' + [regex]::Escape("$CurrentVersion.0") + '(")') `
            -ExpectedCount 1 `
            -Description "application manifest version in $relativePath" `
            -Replacement {
                param($match)
                return $match.Groups[1].Value + "$NewVersion.0" + $match.Groups[2].Value
            }
    }

    foreach ($relativePath in @('README.md', 'docs/windows-packaging.md')) {
        $text = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
        $expectedCount = [regex]::Matches(
            $text,
            [regex]::Escape($CurrentVersion)).Count
        if ($expectedCount -eq 0) {
            throw "No release example version was found in $relativePath."
        }
        $updates[$relativePath] = Replace-RegexExactly `
            -Text $text `
            -Pattern ([regex]::Escape($CurrentVersion)) `
            -ExpectedCount $expectedCount `
            -Description "release example version in $relativePath" `
            -Replacement { param($match) return $NewVersion }
    }

    foreach ($relativePath in $versionFiles) {
        if (-not $updates.Contains($relativePath)) {
            throw "No version updater is registered for $relativePath."
        }
        $currentText = [System.IO.File]::ReadAllText((Get-RepositoryPath $relativePath))
        if ($updates[$relativePath] -ceq $currentText) {
            throw "Version update would not change $relativePath."
        }
    }
    return $updates
}

function Get-GitHubRepository {
    $remote = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('-C', $repositoryRoot, 'remote', 'get-url', 'origin') `
        -CaptureOutput
    $match = [regex]::Match(
        $remote,
        'github\.com(?::|/)(?<repository>[^/\s]+/[^/\s]+?)(?:\.git)?$')
    if (-not $match.Success) {
        throw "origin is not a supported GitHub remote: $remote"
    }
    return $match.Groups['repository'].Value
}

function Get-FourPartExecutableVersion {
    param([Parameter(Mandatory = $true)][string] $Executable)

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Release executable does not exist: $Executable"
    }
    $productVersion = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
    if ($productVersion -notmatch ('^' + [regex]::Escape($Version) + '\.[0-9]+$')) {
        throw "Executable version '$productVersion' does not match '$Version.<build>': $Executable"
    }
    return $productVersion
}

function Get-DownloadPageText {
    param(
        [Parameter(Mandatory = $true)][string] $Text,
        [Parameter(Mandatory = $true)][string] $X64AssetUrl,
        [Parameter(Mandatory = $true)][string] $ArmAssetUrl
    )

    $updated = $Text
    foreach ($asset in @(
            [pscustomobject]@{
                Key = 'windows-x64'
                FileName = "Inkpod-$Version-windows-x64.zip"
                Url = $X64AssetUrl
            },
            [pscustomobject]@{
                Key = 'windows-arm64'
                FileName = "Inkpod-$Version-windows-arm.zip"
                Url = $ArmAssetUrl
            })) {
        $key = [regex]::Escape($asset.Key)
        $updated = Replace-RegexExactly `
            -Text $updated `
            -Pattern ('(<div class="file" data-release-file="' + $key + '">)[^<]*(</div>)') `
            -ExpectedCount 1 `
            -Description "$($asset.Key) download filename" `
            -Replacement {
                param($match)
                return $match.Groups[1].Value + $asset.FileName + $match.Groups[2].Value
            }
        $updated = Replace-RegexExactly `
            -Text $updated `
            -Pattern ('<a class="btn primary" data-release-download="' + $key +
                '" href="[^"]*"(?: aria-disabled="true")?>') `
            -ExpectedCount 1 `
            -Description "$($asset.Key) download link" `
            -Replacement {
                param($match)
                return '<a class="btn primary" data-release-download="' +
                    $asset.Key + '" href="' + $asset.Url + '">'
            }
    }
    return $updated
}

foreach ($tool in @('git.exe')) {
    Assert-CommandAvailable -Name $tool
}

$currentVersion = Get-CurrentProjectVersion
$currentVersionValue = [version]::Parse($currentVersion)
$requestedVersionValue = [version]::Parse($Version)
if ($requestedVersionValue -lt $currentVersionValue) {
    throw "Requested version $Version is older than current version $currentVersion."
}

$repository = Get-GitHubRepository
$tag = "v$Version"
$x64FileName = "Inkpod-$Version-windows-x64.zip"
$armFileName = "Inkpod-$Version-windows-arm.zip"
$x64Archive = Get-RepositoryPath "build/windows-x64-release/package/$x64FileName"
$armArchive = Get-RepositoryPath "build/windows-arm-release/package/$armFileName"
$releaseBaseUrl = "https://github.com/$repository/releases/download/$tag"
$plannedX64Url = "$releaseBaseUrl/$x64FileName"
$plannedArmUrl = "$releaseBaseUrl/$armFileName"
$downloadPagePath = Get-RepositoryPath $downloadPageRelativePath
$downloadPage = [System.IO.File]::ReadAllText($downloadPagePath)
$null = Get-DownloadPageText `
    -Text $downloadPage `
    -X64AssetUrl $plannedX64Url `
    -ArmAssetUrl $plannedArmUrl

$versionUpdates = $null
if ($requestedVersionValue -gt $currentVersionValue) {
    $versionUpdates = Get-VersionFileUpdates `
        -CurrentVersion $currentVersion `
        -NewVersion $Version
}

if ($DryRun) {
    Write-Host "Dry run: Windows prerelease $tag for $repository"
    if ($null -ne $versionUpdates) {
        Write-Host "Would update application version $currentVersion -> $Version in:"
        foreach ($relativePath in $versionUpdates.Keys) {
            Write-Host "  $relativePath"
        }
        Write-Host "> git.exe add -- $($versionFiles -join ' ')"
        Write-Host "> git.exe commit -m `"Release $tag`""
        Write-Host "> git.exe push origin HEAD:$Branch"
    }
    else {
        Write-Host "Application version is already $Version; the version-bump step would be skipped."
    }
    Write-Host '> scripts/build-windows-x64.ps1 -Configuration Release -Clean [-Test on native x64]'
    Write-Host '> scripts/build-windows-arm64.ps1 -Configuration Release -Clean [-Test on native ARM64]'
    Write-Host "Would validate and upload: $x64Archive"
    Write-Host "Would validate and upload: $armArchive"
    Write-Host "> git.exe tag -a $tag -m `"inkpod $tag`""
    Write-Host "> git.exe push origin $tag"
    Write-Host "> gh.exe release create $tag <x64.zip> <arm64.zip> --verify-tag --generate-notes --prerelease"
    Write-Host "Would update $downloadPageRelativePath with:"
    Write-Host "  $plannedX64Url"
    Write-Host "  $plannedArmUrl"
    Write-Host "> git.exe commit -m `"Link Windows downloads for $tag`""
    Write-Host "> git.exe push origin HEAD:$Branch"
    return
}

foreach ($tool in @('gh.exe', 'cargo.exe')) {
    Assert-CommandAvailable -Name $tool
}

$originalLocation = Get-Location
try {
    Set-Location -LiteralPath $repositoryRoot

    $status = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('status', '--porcelain') `
        -CaptureOutput
    if (-not [string]::IsNullOrEmpty($status)) {
        throw 'The working tree is not clean. Commit or remove local changes before publishing.'
    }

    $currentBranch = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('branch', '--show-current') `
        -CaptureOutput
    if ($currentBranch -cne $Branch) {
        throw "Release branch '$Branch' is required, but '$currentBranch' is checked out."
    }

    Invoke-CheckedCommand -Tool 'gh.exe' -Arguments @('auth', 'status')
    Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('fetch', '--tags', 'origin', $Branch)
    $headCommit = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('rev-parse', 'HEAD') `
        -CaptureOutput
    $remoteCommit = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('rev-parse', "origin/$Branch") `
        -CaptureOutput
    if ($headCommit -cne $remoteCommit) {
        throw "HEAD ($headCommit) is not synchronized with origin/$Branch ($remoteCommit)."
    }

    $localTagProbe = Invoke-OptionalCommand `
        -Tool 'git.exe' `
        -Arguments @('rev-parse', '--verify', '--quiet', "refs/tags/$tag^{commit}")
    $localTagCommit = $localTagProbe.Output
    $localTagExists = $localTagProbe.ExitCode -eq 0
    if ($localTagExists) {
        if ($requestedVersionValue -gt $currentVersionValue) {
            throw "Tag $tag already exists at $localTagCommit while the source version is still $currentVersion."
        }
    }

    $releaseProbe = Invoke-OptionalCommand `
        -Tool 'gh.exe' `
        -Arguments @(
            'release',
            'view',
            $tag,
            '--repo',
            $repository,
            '--json',
            'tagName,isPrerelease,url,assets')
    $releaseJson = $releaseProbe.Output
    $releaseExists = $releaseProbe.ExitCode -eq 0
    if ($releaseExists -and $requestedVersionValue -gt $currentVersionValue) {
        throw "GitHub release $tag already exists while the source version is still $currentVersion."
    }

    if ($null -ne $versionUpdates) {
        foreach ($relativePath in $versionUpdates.Keys) {
            Set-TextFileAtomically `
                -Path (Get-RepositoryPath $relativePath) `
                -Text $versionUpdates[$relativePath]
        }

        $null = Invoke-CheckedCommand `
            -Tool 'cargo.exe' `
            -Arguments @('metadata', '--locked', '--format-version', '1', '--no-deps') `
            -CaptureOutput
        $null = Invoke-CheckedCommand `
            -Tool 'cargo.exe' `
            -Arguments @(
                'metadata',
                '--manifest-path',
                'rust/inkpod-format/fuzz/Cargo.toml',
                '--locked',
                '--format-version',
                '1',
                '--no-deps') `
            -CaptureOutput
        Invoke-CheckedCommand -Tool 'git.exe' -Arguments @('diff', '--check')
        Invoke-CheckedCommand -Tool 'git.exe' -Arguments (@('add', '--') + $versionFiles)

        $stagedFiles = @(Invoke-CheckedCommand `
                -Tool 'git.exe' `
                -Arguments @('diff', '--cached', '--name-only') `
                -CaptureOutput) -split "`n"
        $unexpectedFiles = @($stagedFiles | Where-Object {
                -not [string]::IsNullOrWhiteSpace($_) -and $_ -notin $versionFiles
            })
        if ($unexpectedFiles.Count -ne 0) {
            throw "Unexpected staged release files: $($unexpectedFiles -join ', ')"
        }
        foreach ($relativePath in $versionFiles) {
            if ($relativePath -notin $stagedFiles) {
                throw "Expected version file was not staged: $relativePath"
            }
        }

        Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('commit', '-m', "Release $tag")
        Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('push', 'origin', "HEAD:$Branch")
        $headCommit = Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('rev-parse', 'HEAD') `
            -CaptureOutput
    }

    $releaseCommit = $headCommit
    if (-not $releaseExists) {
        if ($localTagExists) {
            if ($localTagCommit -cne $releaseCommit) {
                throw "Existing tag $tag points to $localTagCommit, not release commit $releaseCommit."
            }
        }

        $hostArchitecture = ([string] $env:PROCESSOR_ARCHITECTURE).ToUpperInvariant()
        if ($hostArchitecture -notin @('AMD64', 'ARM64')) {
            throw "Unsupported Windows host architecture: $hostArchitecture"
        }
        $runX64Tests = $hostArchitecture -eq 'AMD64'
        $runArmTests = $hostArchitecture -eq 'ARM64'

        $x64BuildScript = Get-RepositoryPath 'scripts/build-windows-x64.ps1'
        Write-Host "> $x64BuildScript -Configuration Release -Clean -Test:$runX64Tests"
        & $x64BuildScript `
            -Configuration Release `
            -Clean `
            -Test:$runX64Tests

        $armBuildScript = Get-RepositoryPath 'scripts/build-windows-arm64.ps1'
        Write-Host "> $armBuildScript -Configuration Release -Clean -Test:$runArmTests"
        & $armBuildScript `
            -Configuration Release `
            -Clean `
            -Test:$runArmTests

        $x64Executable = Get-RepositoryPath 'build/windows-x64-release/inkpod.exe'
        $armExecutable = Get-RepositoryPath 'build/windows-arm-release/inkpod.exe'
        $x64FourPartVersion = Get-FourPartExecutableVersion -Executable $x64Executable
        $armFourPartVersion = Get-FourPartExecutableVersion -Executable $armExecutable

        $portableVerifier = Get-RepositoryPath 'tests/verify_portable_zip.ps1'
        & $portableVerifier `
            -PackagePath $x64Archive `
            -Executable $x64Executable `
            -Readme (Get-RepositoryPath 'apps/windows/package/README.portable.txt') `
            -License (Get-RepositoryPath 'LICENSE') `
            -ThirdPartyNotices (Get-RepositoryPath 'docs/third-party-notices.md') `
            -ExpectedVersion $Version `
            -ExpectedArchitecture x64 `
            -RunPortableSmoke
        & $portableVerifier `
            -PackagePath $armArchive `
            -Executable $armExecutable `
            -Readme (Get-RepositoryPath 'apps/windows/package/README.portable.txt') `
            -License (Get-RepositoryPath 'LICENSE') `
            -ThirdPartyNotices (Get-RepositoryPath 'docs/third-party-notices.md') `
            -ExpectedVersion $Version `
            -ExpectedArchitecture arm64

        $msixVerifier = Get-RepositoryPath 'tests/verify_msix.ps1'
        & $msixVerifier `
            -PackagePath (Get-RepositoryPath (
                "build/windows-x64-release/package/inkpod-$x64FourPartVersion-x64.msix")) `
            -ExpectedVersion $x64FourPartVersion `
            -ExpectedArchitecture x64
        & $msixVerifier `
            -PackagePath (Get-RepositoryPath (
                "build/windows-arm-release/package/inkpod-$armFourPartVersion-arm64.msix")) `
            -ExpectedVersion $armFourPartVersion `
            -ExpectedArchitecture arm64

        if (-not $localTagExists) {
            Invoke-CheckedCommand `
                -Tool 'git.exe' `
                -Arguments @('tag', '-a', $tag, $releaseCommit, '-m', "inkpod $tag")
            $localTagExists = $true
            $localTagCommit = $releaseCommit
        }
        Invoke-CheckedCommand -Tool 'git.exe' -Arguments @('push', 'origin', $tag)
        Invoke-CheckedCommand `
            -Tool 'gh.exe' `
            -Arguments @(
                'release',
                'create',
                $tag,
                $x64Archive,
                $armArchive,
                '--repo',
                $repository,
                '--target',
                $releaseCommit,
                '--verify-tag',
                '--generate-notes',
                '--title',
                "inkpod $Version",
                '--prerelease')

        $releaseJson = @(& gh.exe release view $tag --repo $repository `
                --json tagName,isPrerelease,url,assets)
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to read the newly created GitHub release $tag."
        }
    }

    $release = (($releaseJson -join "`n") | ConvertFrom-Json)
    if ($release.tagName -cne $tag -or -not $release.isPrerelease) {
        throw "GitHub release $tag is missing or is not a prerelease."
    }
    if (-not $localTagExists) {
        throw "GitHub release $tag exists, but its tag is unavailable locally after fetching tags."
    }
    & git.exe merge-base --is-ancestor $localTagCommit HEAD
    if ($LASTEXITCODE -ne 0) {
        throw "Release tag $tag at $localTagCommit is not an ancestor of the current branch."
    }
    $x64Assets = @($release.assets | Where-Object { $_.name -ceq $x64FileName })
    $armAssets = @($release.assets | Where-Object { $_.name -ceq $armFileName })
    if ($x64Assets.Count -ne 1 -or $armAssets.Count -ne 1) {
        throw "GitHub release $tag does not contain exactly one x64 and one ARM64 portable ZIP."
    }

    $status = Invoke-CheckedCommand `
        -Tool 'git.exe' `
        -Arguments @('status', '--porcelain') `
        -CaptureOutput
    if (-not [string]::IsNullOrEmpty($status)) {
        throw 'The build or release step changed tracked files; refusing to mix them into the download-link commit.'
    }

    $downloadPage = [System.IO.File]::ReadAllText($downloadPagePath)
    $updatedDownloadPage = Get-DownloadPageText `
        -Text $downloadPage `
        -X64AssetUrl $x64Assets[0].url `
        -ArmAssetUrl $armAssets[0].url
    if ($updatedDownloadPage -cne $downloadPage) {
        Set-TextFileAtomically -Path $downloadPagePath -Text $updatedDownloadPage
        Invoke-CheckedCommand -Tool 'git.exe' -Arguments @('diff', '--check')
        Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('add', '--', $downloadPageRelativePath)
        Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('commit', '-m', "Link Windows downloads for $tag")
        Invoke-CheckedCommand `
            -Tool 'git.exe' `
            -Arguments @('push', 'origin', "HEAD:$Branch")
    }

    Write-Host "Published prerelease: $($release.url)"
    Write-Host "x64: $($x64Assets[0].url)"
    Write-Host "ARM64: $($armAssets[0].url)"
}
finally {
    Set-Location -LiteralPath $originalLocation
}
