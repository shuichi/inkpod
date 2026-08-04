param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [Parameter(Mandatory = $true)]
    [string]$Manifest,
    [Parameter(Mandatory = $true)]
    [string]$AssetsDirectory,
    [Parameter(Mandatory = $true)]
    [string]$License,
    [Parameter(Mandatory = $true)]
    [string]$ThirdPartyNotices,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath
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

foreach ($path in @(
        $Executable,
        $Manifest,
        $AssetsDirectory,
        $License,
        $ThirdPartyNotices)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Required MSIX input does not exist: $path"
    }
}

$makeAppx = Find-WindowsSdkTool -Name 'makeappx.exe'
$resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
$outputDirectory = [IO.Path]::GetFullPath((Split-Path -Parent $resolvedOutput))
$layoutDirectory = [IO.Path]::GetFullPath((Join-Path $outputDirectory 'msix-layout'))
$expectedLayout = $outputDirectory.TrimEnd([IO.Path]::DirectorySeparatorChar) +
    [IO.Path]::DirectorySeparatorChar + 'msix-layout'
if (-not $layoutDirectory.Equals($expectedLayout, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to replace unexpected MSIX layout path: $layoutDirectory"
}
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
if (Test-Path -LiteralPath $layoutDirectory) {
    Remove-Item -LiteralPath $layoutDirectory -Recurse -Force
}
New-Item -ItemType Directory -Path $layoutDirectory | Out-Null

Copy-Item -LiteralPath $Executable -Destination (Join-Path $layoutDirectory 'inkpod.exe')
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $layoutDirectory 'AppxManifest.xml')
Copy-Item -LiteralPath $AssetsDirectory -Destination (Join-Path $layoutDirectory 'Assets') -Recurse
Copy-Item -LiteralPath $License -Destination (Join-Path $layoutDirectory 'LICENSE.txt')
Copy-Item -LiteralPath $ThirdPartyNotices -Destination (Join-Path $layoutDirectory 'ThirdPartyNotices.txt')

& $makeAppx pack /d $layoutDirectory /p $resolvedOutput /o /l
if ($LASTEXITCODE -ne 0) {
    throw "makeappx.exe failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path -LiteralPath $resolvedOutput)) {
    throw "makeappx.exe did not create $resolvedOutput"
}
