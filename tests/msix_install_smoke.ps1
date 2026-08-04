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

$packageName = 'inkpod'
$publisher = 'CN=inkpod'
$resolvedPackage = [IO.Path]::GetFullPath($PackagePath)
if (-not (Test-Path -LiteralPath $resolvedPackage)) {
    throw "MSIX does not exist: $resolvedPackage"
}

$principal = [Security.Principal.WindowsPrincipal]::new(
    [Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'MSIX signing/install/uninstall smoke must run from an elevated Windows 11 shell'
}

$operatingSystem = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
if ($operatingSystem.ProductType -ne 1 -or [int]$operatingSystem.BuildNumber -lt 22000) {
    throw "MSIX install/uninstall acceptance requires Windows 11 workstation; found $($operatingSystem.Caption) build $($operatingSystem.BuildNumber)"
}

$preexisting = @(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop)
if ($preexisting.Count -ne 0) {
    throw 'MSIX smoke requires a clean inkpod package state and will not remove an existing installation'
}

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryDirectory = Join-Path $temporaryRoot ("inkpod-msix-smoke-" + [guid]::NewGuid().ToString('N'))
$signedPackage = Join-Path $temporaryDirectory 'inkpod.msix'
$certificatePath = Join-Path $temporaryDirectory 'inkpod-test.cer'
$pfxPath = Join-Path $temporaryDirectory 'inkpod-test.pfx'
$pfxPassword = [guid]::NewGuid().ToString('N')
$certificate = $null
$rsa = $null
$certificateThumbprint = $null
$certificateTrustAttempted = $false
$installedFullName = $null
$packageInstallAttempted = $false

try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    Copy-Item -LiteralPath $resolvedPackage -Destination $signedPackage

    $rsa = [Security.Cryptography.RSA]::Create(2048)
    $request = [Security.Cryptography.X509Certificates.CertificateRequest]::new(
        $publisher,
        $rsa,
        [Security.Cryptography.HashAlgorithmName]::SHA256,
        [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new(
            $false, $false, 0, $true))
    $request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
            [Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature,
            $true))
    $enhancedKeyUsages = [Security.Cryptography.OidCollection]::new()
    $null = $enhancedKeyUsages.Add([Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3'))
    $request.CertificateExtensions.Add(
        [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new(
            $enhancedKeyUsages, $true))
    $certificate = $request.CreateSelfSigned(
        [DateTimeOffset]::Now.AddMinutes(-5),
        [DateTimeOffset]::Now.AddDays(1))
    $certificateThumbprint = $certificate.Thumbprint
    [IO.File]::WriteAllBytes(
        $certificatePath,
        $certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
    [IO.File]::WriteAllBytes(
        $pfxPath,
        $certificate.Export(
            [Security.Cryptography.X509Certificates.X509ContentType]::Pfx,
            $pfxPassword))

    $certificateTrustAttempted = $true
    & certutil.exe -addstore -f Root $certificatePath | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "certutil.exe failed to trust the ephemeral certificate: $LASTEXITCODE"
    }
    $signTool = Find-WindowsSdkTool -Name 'signtool.exe'
    & $signTool sign /fd SHA256 /f $pfxPath /p $pfxPassword /v $signedPackage
    if ($LASTEXITCODE -ne 0) {
        throw "signtool.exe failed with exit code $LASTEXITCODE"
    }

    $packageInstallAttempted = $true
    Add-AppxPackage -Path $signedPackage -ErrorAction Stop
    $installed = @(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop)
    if ($installed.Count -ne 1) {
        throw "Expected exactly one installed inkpod package, found $($installed.Count)"
    }
    $installed = $installed[0]
    $installedFullName = $installed.PackageFullName
    if ($installed.Version.ToString() -ne $ExpectedVersion) {
        throw "Installed version $($installed.Version) does not match $ExpectedVersion"
    }

    foreach ($relativePath in @(
            'inkpod.exe',
            'LICENSE.txt',
            'ThirdPartyNotices.txt',
            'AppxManifest.xml')) {
        $installedPath = Join-Path $installed.InstallLocation $relativePath
        if (-not (Test-Path -LiteralPath $installedPath)) {
            throw "Installed package is missing $relativePath"
        }
    }

    $forbiddenRuntimePattern =
        '^(?:msvcp.*|vcruntime.*|concrt.*|msvcr.*|ucrtbase.*|api-ms-win-crt-.*)\.dll$'
    $runtimeDlls = @(Get-ChildItem -LiteralPath $installed.InstallLocation -Recurse -File |
        Where-Object { $_.Name -match $forbiddenRuntimePattern })
    if ($runtimeDlls.Count -ne 0) {
        throw "Installed package contains dynamic MSVC CRT DLLs: $($runtimeDlls.Name -join ', ')"
    }

    $process = Start-Process `
        -FilePath (Join-Path $installed.InstallLocation 'inkpod.exe') `
        -ArgumentList '--abi-smoke-test' `
        -Wait `
        -PassThru `
        -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "Installed inkpod ABI smoke failed with exit code $($process.ExitCode)"
    }

    Remove-AppxPackage -AllUsers -Package $installedFullName -ErrorAction Stop
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        if (@(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop).Count -eq 0) {
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (@(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop).Count -ne 0) {
        throw 'inkpod package remained registered after uninstall'
    }
    $installedFullName = $null
    $packageInstallAttempted = $false
}
finally {
    $cleanupFailures = [Collections.Generic.List[string]]::new()
    try {
        $cleanupPackages = if ($null -ne $installedFullName) {
            @($installedFullName)
        }
        elseif ($packageInstallAttempted) {
            @(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop |
                ForEach-Object { $_.PackageFullName })
        }
        else {
            @()
        }
        foreach ($cleanupPackage in $cleanupPackages) {
            Remove-AppxPackage -AllUsers -Package $cleanupPackage -ErrorAction Stop
        }
        if ($cleanupPackages.Count -ne 0) {
            for ($attempt = 0; $attempt -lt 50; $attempt++) {
                if (@(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop).Count -eq 0) {
                    break
                }
                Start-Sleep -Milliseconds 100
            }
            if (@(Get-AppxPackage -AllUsers -Name $packageName -ErrorAction Stop).Count -ne 0) {
                $cleanupFailures.Add('inkpod package remained registered during failure cleanup')
            }
        }
    }
    catch {
        $cleanupFailures.Add("failed to uninstall inkpod during cleanup: $($_.Exception.Message)")
    }
    if ($certificateTrustAttempted) {
        & certutil.exe -delstore Root $certificateThumbprint 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) {
            $cleanupFailures.Add(
                "certutil.exe failed to remove the ephemeral certificate: $LASTEXITCODE")
        }
    }
    if ($null -ne $certificate) {
        $certificate.Dispose()
    }
    if ($null -ne $rsa) {
        $rsa.Dispose()
    }
    if (Test-Path -LiteralPath $temporaryDirectory) {
        $resolvedTemporary = [IO.Path]::GetFullPath($temporaryDirectory)
        $expectedPrefix = $temporaryRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) +
            [IO.Path]::DirectorySeparatorChar + 'inkpod-msix-smoke-'
        if (-not $resolvedTemporary.StartsWith(
                $expectedPrefix,
                [StringComparison]::OrdinalIgnoreCase)) {
            $cleanupFailures.Add("Refusing to remove unexpected temporary path: $resolvedTemporary")
        }
        else {
            try {
                Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force -ErrorAction Stop
            }
            catch {
                $cleanupFailures.Add("failed to remove temporary package state: $($_.Exception.Message)")
            }
        }
    }
    if ($cleanupFailures.Count -ne 0) {
        throw ($cleanupFailures -join '; ')
    }
}
