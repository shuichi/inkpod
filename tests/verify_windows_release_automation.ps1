param(
    [Parameter(Mandatory = $true)]
    [string] $RepositoryRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resolvedRoot = [System.IO.Path]::GetFullPath($RepositoryRoot)
$scriptPath = Join-Path $resolvedRoot 'scripts\publish-windows-release.ps1'
$cmakePath = Join-Path $resolvedRoot 'CMakeLists.txt'
$downloadPagePath = Join-Path $resolvedRoot 'html\index.html'

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][string] $Path)

    $stream = [System.IO.File]::OpenRead($Path)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString(
                $sha256.ComputeHash($stream))).Replace('-', '')
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

foreach ($path in @($scriptPath, $cmakePath, $downloadPagePath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required release-automation input does not exist: $path"
    }
}

$parseErrors = $null
$tokens = $null
[System.Management.Automation.Language.Parser]::ParseFile(
    $scriptPath,
    [ref] $tokens,
    [ref] $parseErrors) | Out-Null
if ($parseErrors.Count -ne 0) {
    throw "Release script has PowerShell parse errors: $($parseErrors.Message -join '; ')"
}

$scriptAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $scriptPath,
    [ref] $tokens,
    [ref] $parseErrors)
$optionalCommandFunction = @($scriptAst.FindAll({
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] `
                -and $node.Name -ceq 'Invoke-OptionalCommand'
        }, $true))
if ($optionalCommandFunction.Count -ne 1) {
    throw 'Release script must define exactly one Invoke-OptionalCommand function.'
}
Invoke-Expression $optionalCommandFunction[0].Extent.Text
$missingTagName = '__inkpod_release_automation_missing_tag__'
$stderrProbe = Invoke-OptionalCommand `
    -Tool 'git.exe' `
    -Arguments @(
        '-C',
        $resolvedRoot,
        'rev-parse',
        "refs/tags/$missingTagName^{commit}")
if ($stderrProbe.ExitCode -eq 0) {
    throw 'A failing native probe was not returned as an optional-command miss.'
}
$missingTagProbe = Invoke-OptionalCommand `
    -Tool 'git.exe' `
    -Arguments @(
        '-C',
        $resolvedRoot,
        'rev-parse',
        '--verify',
        '--quiet',
        "refs/tags/$missingTagName^{commit}")
if ($missingTagProbe.ExitCode -eq 0 `
        -or -not [string]::IsNullOrEmpty($missingTagProbe.Output)) {
    throw 'A missing release tag was not reported as a quiet optional-command miss.'
}

$cmake = [System.IO.File]::ReadAllText($cmakePath)
$versionMatch = [regex]::Match(
    $cmake,
    '(?m)^project\(inkpod VERSION (?<major>[0-9]+)\.(?<minor>[0-9]+)\.(?<patch>[0-9]+) LANGUAGES C CXX\)\r?$')
if (-not $versionMatch.Success) {
    throw 'Unable to read the current CMake project version.'
}
$nextPatch = [uint64]::Parse($versionMatch.Groups['patch'].Value) + 1
$candidateVersion = '{0}.{1}.{2}' -f `
    $versionMatch.Groups['major'].Value,
    $versionMatch.Groups['minor'].Value,
    $nextPatch

$trackedInputs = @(
    'CMakeLists.txt',
    'Cargo.toml',
    'Cargo.lock',
    'rust\inkpod-format\fuzz\Cargo.lock',
    'apps\windows\package\Package.appxmanifest',
    'apps\windows\app\app.manifest',
    'README.md',
    'docs\windows-packaging.md',
    'html\index.html'
)
$beforeHashes = @{}
foreach ($relativePath in $trackedInputs) {
    $path = Join-Path $resolvedRoot $relativePath
    $beforeHashes[$relativePath] = Get-Sha256Hex -Path $path
}

$output = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass `
        -File $scriptPath `
        -Version $candidateVersion `
        -DryRun 2>&1)
if ($LASTEXITCODE -ne 0) {
    throw "Release dry run failed with exit code ${LASTEXITCODE}: $($output -join "`n")"
}
$outputText = $output -join "`n"
foreach ($requiredText in @(
        "Dry run: Windows prerelease v$candidateVersion",
        'scripts/build-windows-x64.ps1',
        'scripts/build-windows-arm64.ps1',
        'gh.exe release create',
        "Inkpod-$candidateVersion-windows-x64.zip",
        "Inkpod-$candidateVersion-windows-arm.zip",
        'html/index.html')) {
    if ($outputText.IndexOf($requiredText, [StringComparison]::Ordinal) -lt 0) {
        throw "Release dry run did not report expected text: $requiredText"
    }
}

foreach ($relativePath in $trackedInputs) {
    $path = Join-Path $resolvedRoot $relativePath
    $afterHash = Get-Sha256Hex -Path $path
    if ($afterHash -cne $beforeHashes[$relativePath]) {
        throw "Release dry run modified a tracked input: $relativePath"
    }
}
