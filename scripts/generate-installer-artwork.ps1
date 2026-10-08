$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$logo = [System.Drawing.Bitmap]::FromFile((Join-Path $root 'public/strawberrydisk.png'))
$output = Join-Path $root 'src-tauri/icons/nsis'

function New-InstallerBitmap([int]$width, [int]$height, [string]$path, [System.Drawing.Color]$topColor, [System.Drawing.Color]$bottomColor, [System.Drawing.Rectangle]$logoBounds) {
    $bitmap = [System.Drawing.Bitmap]::new($width, $height, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
            $bounds = [System.Drawing.Rectangle]::new(0, 0, $width, $height)
            $background = [System.Drawing.Drawing2D.LinearGradientBrush]::new($bounds, $topColor, $bottomColor, [System.Drawing.Drawing2D.LinearGradientMode]::Vertical)
            try { $graphics.FillRectangle($background, $bounds) } finally { $background.Dispose() }
            $graphics.DrawImage($logo, $logoBounds)
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Bmp)
    } finally {
        $bitmap.Dispose()
    }
}

try {
    New-InstallerBitmap 164 314 (Join-Path $output 'installer-sidebar.bmp') ([System.Drawing.Color]::FromArgb(255, 250, 247)) ([System.Drawing.Color]::FromArgb(255, 230, 229)) ([System.Drawing.Rectangle]::new(7, 82, 150, 150))
    New-InstallerBitmap 150 57 (Join-Path $output 'installer-header.bmp') ([System.Drawing.Color]::White) ([System.Drawing.Color]::White) ([System.Drawing.Rectangle]::new(103, 5, 46, 46))
} finally {
    $logo.Dispose()
}
