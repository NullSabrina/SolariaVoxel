param(
  [string]$Root = (Split-Path -Parent $PSScriptRoot)
)

Add-Type -AssemblyName System.Drawing

$size = 32
$cols = 8
$tiles = 19
$rows = [math]::Ceiling($tiles / $cols)
$width = $cols * $size
$height = $rows * $size
$srcDir = Join-Path $Root "assets/src/textures/minetest"
$out = Join-Path $Root "assets/atlas.png"

$map = @{
  0  = "default_grass.png"
  1  = "default_grass_side.png"
  2  = "default_dirt.png"
  3  = "default_stone.png"
  4  = "default_sand.png"
  5  = "default_tree.png"
  6  = "default_tree_top.png"
  7  = "default_leaves.png"
  8  = "default_torch_on_floor.png"
  9  = "default_snow.png"
  10 = "default_water.png"
  11 = "default_wood.png"
  14 = "default_dry_dirt.png"
  15 = "default_gravel.png"
  17 = "default_lava.png"
  18 = "default_obsidian.png"
}

$bmp = New-Object System.Drawing.Bitmap $width, $height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
$g.Clear([System.Drawing.Color]::Transparent)

function Draw-Tile([int]$tile, [string]$file) {
  $path = Join-Path $srcDir $file
  if (-not (Test-Path -LiteralPath $path)) { return }
  $img = [System.Drawing.Image]::FromFile($path)
  $x = ($tile % $cols) * $size
  $y = [math]::Floor($tile / $cols) * $size
  $g.DrawImage($img, $x, $y, $size, $size)
  $img.Dispose()
}

foreach ($k in $map.Keys) { Draw-Tile $k $map[$k] }

# Cara lateral del bloque de hierba: Minetest entrega `default_grass_side` como
# OVERLAY transparente (se compone sobre `default_dirt`). Dibujarla sola dejaba
# el bloque con hierba arriba y el resto hueco, asi que aqui se compone la
# tierra opaca debajo y la hierba encima.
$grassSide = Join-Path $srcDir "default_grass_side.png"
$dirtSide = Join-Path $srcDir "default_dirt.png"
if ((Test-Path -LiteralPath $grassSide) -and (Test-Path -LiteralPath $dirtSide)) {
  $x = (1 % $cols) * $size
  $y = [math]::Floor(1 / $cols) * $size
  $base = [System.Drawing.Image]::FromFile($dirtSide)
  $g.DrawImage($base, $x, $y, $size, $size)
  $base.Dispose()
  $top = [System.Drawing.Image]::FromFile($grassSide)
  $g.DrawImage($top, $x, $y, $size, $size)
  $top.Dispose()
}

# Mesa de crafteo: tablones + marco/rejilla.
$wood = Join-Path $srcDir "default_wood.png"
if (Test-Path -LiteralPath $wood) {
  $img = [System.Drawing.Image]::FromFile($wood)
  foreach ($tile in 12, 13) {
    $x = ($tile % $cols) * $size
    $y = [math]::Floor($tile / $cols) * $size
    $g.DrawImage($img, $x, $y, $size, $size)
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 70, 48, 30)), 2
    $g.DrawRectangle($pen, $x + 1, $y + 1, $size - 3, $size - 3)
    if ($tile -eq 13) {
      $g.DrawLine($pen, $x + 10, $y + 1, $x + 10, $y + $size - 2)
      $g.DrawLine($pen, $x + 21, $y + 1, $x + 21, $y + $size - 2)
      $g.DrawLine($pen, $x + 1, $y + 10, $x + $size - 2, $y + 10)
      $g.DrawLine($pen, $x + 1, $y + 21, $x + $size - 2, $y + 21)
    }
    $pen.Dispose()
  }
  $img.Dispose()
}

# Podzol: tierra + motas de musgo.
$dirt = Join-Path $srcDir "default_dirt.png"
if (Test-Path -LiteralPath $dirt) {
  $img = [System.Drawing.Image]::FromFile($dirt)
  $x0 = 16 % $cols * $size
  $y0 = [math]::Floor(16 / $cols) * $size
  $g.DrawImage($img, $x0, $y0, $size, $size)
  $img.Dispose()
  for ($y = 0; $y -lt $size; $y++) {
    for ($x = 0; $x -lt $size; $x++) {
      $h = ($x * 37 + $y * 61) % 97
      if ($h -lt 5) { $bmp.SetPixel($x0 + $x, $y0 + $y, [System.Drawing.Color]::FromArgb(255, 92, 120, 52)) }
      elseif ($h -lt 10) { $bmp.SetPixel($x0 + $x, $y0 + $y, [System.Drawing.Color]::FromArgb(255, 112, 74, 40)) }
    }
  }
}

$g.Dispose()

# Alpha: agua translucida y hojas con huecos de cutout.
$rect = New-Object System.Drawing.Rectangle 0, 0, $width, $height
$data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadWrite, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$stride = $data.Stride
$bytes = New-Object byte[] ($stride * $height)
[System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
function SetAlpha([int]$tile, [int]$a) {
  $ox = ($tile % $cols) * $size
  $oy = [math]::Floor($tile / $cols) * $size
  for ($y = 0; $y -lt $size; $y++) {
    for ($x = 0; $x -lt $size; $x++) {
      $i = ($oy + $y) * $stride + ($ox + $x) * 4 + 3
      $bytes[$i] = $a
    }
  }
}
SetAlpha 10 200
# Hojas: la fuente CC0 trae ~37% de huecos y el cutout se veia demasiado
# transparente. Rellenamos la mayoria con verdes oscuros de la propia hoja y
# conservamos ~15% de huecos (por pixel-fuente, en bloques de 2x2) para que la
# copa se lea frondosa sin parecer una reja.
$ox = (7 % $cols) * $size
$oy = [math]::Floor(7 / $cols) * $size
$leafFill = @(
  @(24, 34, 22),
  @(28, 44, 26),
  @(32, 55, 29),
  @(38, 62, 32)
)
for ($y = 0; $y -lt $size; $y++) {
  for ($x = 0; $x -lt $size; $x++) {
    $i = ($oy + $y) * $stride + ($ox + $x) * 4
    $a = $bytes[$i + 3]
    $bx = [math]::Floor($x / 2)
    $by = [math]::Floor($y / 2)
    $h = ($bx * 53 + $by * 97) % 100
    if ($a -eq 0) {
      if ($h -lt 40) { continue }
      $c = $leafFill[$h % $leafFill.Count]
      $bytes[$i] = $c[2]
      $bytes[$i + 1] = $c[1]
      $bytes[$i + 2] = $c[0]
      $bytes[$i + 3] = 255
    } elseif ($a -lt 255) {
      $bytes[$i + 3] = 255
    }
  }
}
[System.Runtime.InteropServices.Marshal]::Copy($bytes, 0, $data.Scan0, $bytes.Length)
$bmp.UnlockBits($data)
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "[atlas] $out ($width x $height) desde Minetest CC0"

