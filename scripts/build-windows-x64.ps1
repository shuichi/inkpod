[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string[]] $Configuration = @("Debug", "Release"),

    [switch] $Clean,
    [switch] $Test,
    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$maximumBuildNumber = 65535
$repositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path -Path $PSScriptRoot -ChildPath ".."))
$stateDirectory = Join-Path -Path $repositoryRoot -ChildPath ".inkpod-local"
$counterPath = Join-Path -Path $stateDirectory -ChildPath "build-number.txt"
$lockPath = Join-Path -Path $stateDirectory -ChildPath "build-number.lock"
$selectedConfigurations = @($Configuration | Select-Object -Unique)

function Get-ValidatedBuildNumber {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Text,

        [Parameter(Mandatory = $true)]
        [string] $Source
    )

    $trimmed = $Text.Trim()
    [uint32] $parsed = 0
    if (-not [uint32]::TryParse($trimmed, [ref] $parsed) -or
        $parsed -gt $maximumBuildNumber) {
        throw "Invalid Inkpod build number in ${Source}: '${trimmed}'. Expected 0-$maximumBuildNumber."
    }
    return [int] $parsed
}

function Get-HighestCachedBuildNumber {
    $buildRoot = Join-Path -Path $repositoryRoot -ChildPath "build"
    if (-not (Test-Path -LiteralPath $buildRoot -PathType Container)) {
        return 0
    }

    $highest = 0
    $x64BuildDirectories = @("windows-x64-debug", "windows-x64-release")
    $cacheFiles = Get-ChildItem -LiteralPath $buildRoot -Filter "CMakeCache.txt" `
        -Recurse -File -ErrorAction SilentlyContinue
    foreach ($cacheFile in $cacheFiles) {
        if ($cacheFile.Directory.Name -notin $x64BuildDirectories) {
            continue
        }

        $match = [regex]::Match(
            [System.IO.File]::ReadAllText($cacheFile.FullName),
            "(?m)^INKPOD_BUILD_NUMBER:STRING=(?<number>[0-9]+)$")
        if (-not $match.Success) {
            continue
        }

        $cached = Get-ValidatedBuildNumber `
            -Text $match.Groups["number"].Value `
            -Source $cacheFile.FullName
        if ($cached -gt $highest) {
            $highest = $cached
        }
    }
    return $highest
}

function Get-CurrentBuildNumber {
    $current = Get-HighestCachedBuildNumber
    if (Test-Path -LiteralPath $counterPath -PathType Leaf) {
        $stored = Get-ValidatedBuildNumber `
            -Text ([System.IO.File]::ReadAllText($counterPath)) `
            -Source $counterPath
        if ($stored -gt $current) {
            $current = $stored
        }
    }
    return $current
}

function Set-BuildNumberAtomically {
    param(
        [Parameter(Mandatory = $true)]
        [int] $Value
    )

    $temporaryPath = "$counterPath.$PID.tmp"
    $backupPath = "$counterPath.backup"
    try {
        $encoding = [System.Text.UTF8Encoding]::new($false)
        [System.IO.File]::WriteAllText($temporaryPath, "$Value`r`n", $encoding)
        if (Test-Path -LiteralPath $counterPath -PathType Leaf) {
            if (Test-Path -LiteralPath $backupPath -PathType Leaf) {
                Remove-Item -LiteralPath $backupPath -Force
            }
            [System.IO.File]::Replace(
                $temporaryPath,
                $counterPath,
                $backupPath,
                $true)
            Remove-Item -LiteralPath $backupPath -Force
        } else {
            [System.IO.File]::Move($temporaryPath, $counterPath)
        }
    } finally {
        if (Test-Path -LiteralPath $temporaryPath -PathType Leaf) {
            Remove-Item -LiteralPath $temporaryPath -Force
        }
    }
}

function Resolve-VsDevCmd {
    if ($env:VSCMD_ARG_TGT_ARCH -eq "x64" -and
        (Get-Command -Name "cl.exe" -CommandType Application `
            -ErrorAction SilentlyContinue)) {
        return $null
    }

    $programFilesX86 = [System.Environment]::GetFolderPath("ProgramFilesX86")
    $vswherePath = Join-Path -Path $programFilesX86 `
        -ChildPath "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswherePath -PathType Leaf)) {
        throw "Visual Studio locator not found: $vswherePath"
    }

    $installationPaths = @(& $vswherePath `
            -latest `
            -products "*" `
            -requires "Microsoft.VisualStudio.Component.VC.Tools.x86.x64" `
            -property "installationPath")
    $vswhereExitCode = $LASTEXITCODE
    $installationPath = $installationPaths | Select-Object -First 1
    if ($vswhereExitCode -ne 0 -or
        [string]::IsNullOrWhiteSpace($installationPath)) {
        throw "No Visual Studio installation with the x64 MSVC toolchain was found."
    }

    $developerCommand = Join-Path -Path $installationPath `
        -ChildPath "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path -LiteralPath $developerCommand -PathType Leaf)) {
        throw "Visual Studio developer command file not found: $developerCommand"
    }
    return $developerCommand
}

function ConvertTo-CmdToken {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Token
    )

    if ($Token.Contains('"')) {
        throw "A command token contains an unsupported quote: $Token"
    }
    return '"' + $Token + '"'
}

function Resolve-BuildToolPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Tool,

        [AllowNull()]
        [string] $DeveloperCommand
    )

    $commands = @(Get-Command -Name $Tool -CommandType Application `
        -ErrorAction SilentlyContinue)
    if ($commands.Count -gt 0) {
        return [string] $commands[0].Source
    }
    if (-not [string]::IsNullOrEmpty($DeveloperCommand)) {
        return $Tool
    }
    throw "Required build tool was not found on PATH: $Tool"
}

function Invoke-BuildTool {
    param(
        [Parameter(Mandatory = $true)]
        [string] $ToolPath,

        [Parameter(Mandatory = $true)]
        [string] $DisplayName,

        [Parameter(Mandatory = $true)]
        [string[]] $Arguments,

        [AllowNull()]
        [string] $DeveloperCommand
    )

    Write-Host "> $DisplayName $($Arguments -join ' ')"

    if ([string]::IsNullOrEmpty($DeveloperCommand)) {
        & $ToolPath @Arguments
    } else {
        $tokens = @($ToolPath) + $Arguments |
            ForEach-Object { ConvertTo-CmdToken -Token $_ }
        $commandLine = 'call "' + $DeveloperCommand +
            '" -no_logo -arch=x64 -host_arch=x64 && ' +
            ($tokens -join ' ')
        & $env:ComSpec /d /s /c $commandLine
    }

    if ($LASTEXITCODE -ne 0) {
        throw "$DisplayName failed with exit code $LASTEXITCODE. Build number remains reserved."
    }
}

if ($selectedConfigurations.Count -eq 0) {
    throw "At least one configuration is required."
}

$developerCommand = Resolve-VsDevCmd
$cmakeToolPath = Resolve-BuildToolPath `
    -Tool "cmake.exe" `
    -DeveloperCommand $developerCommand
$ctestToolPath = $null
if ($Test) {
    $ctestToolPath = Resolve-BuildToolPath `
        -Tool "ctest.exe" `
        -DeveloperCommand $developerCommand
}

if ($DryRun) {
    $currentBuildNumber = Get-CurrentBuildNumber
    if ($currentBuildNumber -ge $maximumBuildNumber) {
        throw "Inkpod build number $currentBuildNumber cannot be incremented; the maximum is $maximumBuildNumber."
    }
    Write-Host "Dry run: would reserve Inkpod build number $($currentBuildNumber + 1)."
    Write-Host "Configurations: $($selectedConfigurations -join ', ')"
    Write-Host "CMake: $cmakeToolPath"
    if ($Test) {
        Write-Host "CTest: $ctestToolPath"
    }
    return
}

New-Item -ItemType Directory -Path $stateDirectory -Force | Out-Null
$lockStream = $null
$originalLocation = Get-Location
try {
    try {
        $lockStream = [System.IO.File]::Open(
            $lockPath,
            [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None)
    } catch [System.IO.IOException] {
        throw "Another Inkpod local build is using the build-number lock: $lockPath"
    }

    $currentBuildNumber = Get-CurrentBuildNumber
    if ($currentBuildNumber -ge $maximumBuildNumber) {
        throw "Inkpod build number $currentBuildNumber cannot be incremented; the maximum is $maximumBuildNumber."
    }
    $nextBuildNumber = $currentBuildNumber + 1

    Set-BuildNumberAtomically -Value $nextBuildNumber
    Write-Host "Reserved Inkpod build number $nextBuildNumber in $counterPath"

    Set-Location -LiteralPath $repositoryRoot
    foreach ($selectedConfiguration in $selectedConfigurations) {
        $preset = "windows-x64-$($selectedConfiguration.ToLowerInvariant())"
        Invoke-BuildTool `
            -ToolPath $cmakeToolPath `
            -DisplayName "cmake.exe" `
            -Arguments @(
                "--preset",
                $preset,
                "-DINKPOD_BUILD_NUMBER=$nextBuildNumber") `
            -DeveloperCommand $developerCommand

        $buildArguments = @("--build", "--preset", $preset)
        if ($Clean) {
            $buildArguments += "--clean-first"
        }
        Invoke-BuildTool `
            -ToolPath $cmakeToolPath `
            -DisplayName "cmake.exe" `
            -Arguments $buildArguments `
            -DeveloperCommand $developerCommand

        if ($Test) {
            Invoke-BuildTool `
                -ToolPath $ctestToolPath `
                -DisplayName "ctest.exe" `
                -Arguments @("--preset", $preset) `
                -DeveloperCommand $developerCommand
        }
    }

    Write-Host "Completed Inkpod x64 build $nextBuildNumber."
} finally {
    Set-Location -LiteralPath $originalLocation
    if ($null -ne $lockStream) {
        $lockStream.Dispose()
    }
}
