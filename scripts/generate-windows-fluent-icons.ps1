param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase

$iconRoot = Join-Path $RepositoryRoot 'apps/windows/ui/icons/fluent'
$manifestPath = Join-Path $iconRoot 'selected-icons.tsv'
$sourceRoot = Join-Path $iconRoot 'svg'
$outputPath = Join-Path $iconRoot 'fluent_icon_masks.bin'
$maskSize = 48
$supersample = 4
$renderSize = $maskSize * $supersample

$entries = @()
foreach ($line in [System.IO.File]::ReadAllLines($manifestPath)) {
    if ([string]::IsNullOrWhiteSpace($line) -or $line.StartsWith('#')) {
        continue
    }
    $fields = $line.Split('|')
    if ($fields.Count -ne 5) {
        throw "Malformed icon manifest row: $line"
    }
    $entries += [pscustomobject]@{
        Index = [int]$fields[0]
        SemanticKey = $fields[1]
        SourceFile = $fields[3]
        Sha256 = $fields[4]
    }
}
if ($entries.Count -ne 24) {
    throw "Expected 24 selected icons, found $($entries.Count)"
}

$payload = [System.Collections.Generic.List[byte]]::new($entries.Count * $maskSize * $maskSize)
$invariant = [System.Globalization.CultureInfo]::InvariantCulture
$oldCulture = [System.Threading.Thread]::CurrentThread.CurrentCulture
[System.Threading.Thread]::CurrentThread.CurrentCulture = $invariant
try {
    for ($entryIndex = 0; $entryIndex -lt $entries.Count; ++$entryIndex) {
        $entry = $entries[$entryIndex]
        if ($entry.Index -ne $entryIndex) {
            throw "Atlas index is not contiguous at $($entry.SemanticKey)"
        }
        $sourcePath = Join-Path $sourceRoot $entry.SourceFile
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePath).Hash.ToLowerInvariant()
        if ($actualHash -ne $entry.Sha256) {
            throw "SVG hash mismatch for $($entry.SourceFile): $actualHash"
        }
        [xml]$document = [System.IO.File]::ReadAllText($sourcePath)
        $viewBox = ([string]$document.svg.viewBox).Split(
            @(' ', ','), [System.StringSplitOptions]::RemoveEmptyEntries)
        if ($viewBox.Count -ne 4) {
            throw "Unsupported SVG viewBox in $($entry.SourceFile)"
        }
        $viewLeft = [double]::Parse($viewBox[0], $invariant)
        $viewTop = [double]::Parse($viewBox[1], $invariant)
        $viewWidth = [double]::Parse($viewBox[2], $invariant)
        $viewHeight = [double]::Parse($viewBox[3], $invariant)
        if ($viewWidth -le 0.0 -or $viewHeight -le 0.0) {
            throw "Invalid SVG viewBox in $($entry.SourceFile)"
        }

        $visual = [System.Windows.Media.DrawingVisual]::new()
        $drawing = $visual.RenderOpen()
        $scale = [Math]::Min($renderSize / $viewWidth, $renderSize / $viewHeight)
        $offsetX = ($renderSize - $viewWidth * $scale) / 2.0 - $viewLeft * $scale
        $offsetY = ($renderSize - $viewHeight * $scale) / 2.0 - $viewTop * $scale
        $transform = [System.Windows.Media.TransformGroup]::new()
        $transform.Children.Add([System.Windows.Media.ScaleTransform]::new($scale, $scale))
        $transform.Children.Add([System.Windows.Media.TranslateTransform]::new($offsetX, $offsetY))
        foreach ($path in @($document.svg.path)) {
            $geometry = [System.Windows.Media.PathGeometry]::CreateFromGeometry(
                [System.Windows.Media.Geometry]::Parse([string]$path.d))
            $geometry.FillRule = [System.Windows.Media.FillRule]::Nonzero
            $geometry.Transform = $transform
            $drawing.DrawGeometry([System.Windows.Media.Brushes]::White, $null, $geometry)
        }
        $drawing.Close()

        $bitmap = [System.Windows.Media.Imaging.RenderTargetBitmap]::new(
            $renderSize,
            $renderSize,
            96.0,
            96.0,
            [System.Windows.Media.PixelFormats]::Pbgra32)
        $bitmap.Render($visual)
        $pixels = [byte[]]::new($renderSize * $renderSize * 4)
        $bitmap.CopyPixels($pixels, $renderSize * 4, 0)
        for ($y = 0; $y -lt $maskSize; ++$y) {
            for ($x = 0; $x -lt $maskSize; ++$x) {
                $alpha = 0
                for ($sampleY = 0; $sampleY -lt $supersample; ++$sampleY) {
                    for ($sampleX = 0; $sampleX -lt $supersample; ++$sampleX) {
                        $sourceX = $x * $supersample + $sampleX
                        $sourceY = $y * $supersample + $sampleY
                        $alpha += $pixels[(($sourceY * $renderSize + $sourceX) * 4) + 3]
                    }
                }
                $payload.Add([byte][Math]::Round(
                    $alpha / [double]($supersample * $supersample),
                    [MidpointRounding]::AwayFromZero))
            }
        }
    }
} finally {
    [System.Threading.Thread]::CurrentThread.CurrentCulture = $oldCulture
}

$fnv = [uint64]2166136261
foreach ($value in $payload) {
    $fnv = (($fnv -bxor [uint64]$value) * [uint64]16777619) -band [uint64]4294967295
}

$temporary = "$outputPath.tmp"
$stream = [System.IO.File]::Open(
    $temporary,
    [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::Write,
    [System.IO.FileShare]::None)
try {
    $writer = [System.IO.BinaryWriter]::new($stream)
    $writer.Write([System.Text.Encoding]::ASCII.GetBytes('INKPODIA'))
    $writer.Write([uint16]1)
    $writer.Write([uint16]$maskSize)
    $writer.Write([uint16]$maskSize)
    $writer.Write([uint16]$entries.Count)
    $writer.Write([uint32]$payload.Count)
    $writer.Write([uint32]$fnv)
    $writer.Write($payload.ToArray())
    $writer.Flush()
} finally {
    $stream.Dispose()
}
Move-Item -Force -LiteralPath $temporary -Destination $outputPath

$outputHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $outputPath).Hash.ToLowerInvariant()
Write-Output "Generated $outputPath"
Write-Output "SHA-256 $outputHash"
