param(
    [Parameter(Mandatory = $true)]
    [string]$PackagePath,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedVersion
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Find-WindowsSdkTool {
    param([Parameter(Mandatory = $true)][string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $candidates = @(Get-ChildItem -Path $kitsRoot -Filter $Name -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq 'x64' } |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending)
    if ($candidates.Count -eq 0) {
        throw "$Name was not found in PATH or the Windows 10/11 SDK"
    }
    return $candidates[0].FullName
}

$resolvedPackage = [IO.Path]::GetFullPath($PackagePath)
if (-not (Test-Path -LiteralPath $resolvedPackage -PathType Leaf)) {
    throw "MSIX does not exist: $resolvedPackage"
}

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryPrefix = $temporaryRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) +
    [IO.Path]::DirectorySeparatorChar + 'inkpod-msix-payload-'
$temporaryDirectory = $temporaryPrefix + [guid]::NewGuid().ToString('N')

try {
    $makeAppx = Find-WindowsSdkTool -Name 'makeappx.exe'
    & $makeAppx unpack /p $resolvedPackage /d $temporaryDirectory /o /l
    if ($LASTEXITCODE -ne 0) {
        throw "makeappx.exe failed to unpack the MSIX: $LASTEXITCODE"
    }

    foreach ($relativePath in @(
            'inkpod.exe',
            'LICENSE.txt',
            'ThirdPartyNotices.txt',
            'AppxManifest.xml',
            'msvcp140.dll',
            'vcruntime140.dll',
            'vcruntime140_1.dll')) {
        $payloadPath = Join-Path $temporaryDirectory $relativePath
        if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf)) {
            throw "MSIX payload is missing $relativePath"
        }
    }

    $manifest = Get-Content -Raw -LiteralPath (Join-Path $temporaryDirectory 'AppxManifest.xml')
    foreach ($requiredText in @(
            "Version=`"$ExpectedVersion`"",
            'ProcessorArchitecture="x64"',
            'Executable="inkpod.exe"')) {
        if ($manifest.IndexOf($requiredText, [StringComparison]::Ordinal) -lt 0) {
            throw "MSIX manifest is missing $requiredText"
        }
    }
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        $resolvedTemporary = [IO.Path]::GetFullPath($temporaryDirectory)
        if (-not $resolvedTemporary.StartsWith(
                $temporaryPrefix,
                [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove unexpected temporary path: $resolvedTemporary"
        }
        Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force
    }
}
