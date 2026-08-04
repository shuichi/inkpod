param(
    [Parameter(Mandatory = $true)]
    [string] $Executable,

    [Parameter(Mandatory = $true)]
    [string] $Dumpbin
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($path in @($Executable, $Dumpbin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Static CRT verification input does not exist: $path"
    }
}

$dependencyOutput = & $Dumpbin /DEPENDENTS $Executable 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin /DEPENDENTS failed for '$Executable':`n$dependencyOutput"
}

$dependencies = @(
    [System.Text.RegularExpressions.Regex]::Matches(
        $dependencyOutput,
        '(?i)(?<![A-Za-z0-9_.-])([A-Za-z0-9_.-]+\.dll)(?![A-Za-z0-9_.-])') |
        ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() } |
        Sort-Object -Unique
)
if ($dependencies.Count -eq 0) {
    throw "dumpbin output did not expose any executable DLL dependencies:`n$dependencyOutput"
}

$forbiddenRuntimePattern =
    '^(?:msvcp.*|vcruntime.*|concrt.*|msvcr.*|ucrtbase.*|api-ms-win-crt-.*)\.dll$'
$forbiddenDependencies = @($dependencies | Where-Object {
        $_ -match $forbiddenRuntimePattern
    })
if ($forbiddenDependencies.Count -ne 0) {
    throw "Executable imports dynamic MSVC CRT DLLs: $($forbiddenDependencies -join ', ')"
}

Write-Host "Static MSVC CRT verified. Direct system DLLs: $($dependencies -join ', ')"
