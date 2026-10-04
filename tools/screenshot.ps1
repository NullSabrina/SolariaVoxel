# tools/screenshot.ps1
#
# Lanza un ejecutable, espera a que aparezca su ventana y guarda una captura
# de esa ventana en un PNG. Sirve para dejar evidencia visual de cada hito
# (v0.1.0, v0.1.1, ...) sin depender de herramientas externas.
#
# Por que "siempre encima": Windows bloquea SetForegroundWindow si el proceso
# que llama no tiene el foco (proteccion anti-robo-de-foco), asi que una ventana
# tapada saldria en la captura ocupando su sitio en pantalla. Poniendo la
# ventana temporalmente HWND_TOPMOST garantizamos que se vea la nuestra.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File tools/screenshot.ps1 `
#       -Exe target\debug\solaria_voxel.exe -Out screenshots\v0.1.0.png

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$WaitSeconds = 4
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public static class Win32 {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);

    public static readonly IntPtr HWND_TOPMOST = new IntPtr(-1);
    public static readonly IntPtr HWND_NOTOPMOST = new IntPtr(-2);
    public const uint SWP_NOSIZE = 0x0001;
    public const uint SWP_NOMOVE = 0x0002;
    public const uint SWP_SHOWWINDOW = 0x0040;
    public const int SW_RESTORE = 9;

    public static string Title(IntPtr hWnd) {
        var sb = new StringBuilder(512);
        GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }
}
"@

$proc = Start-Process -FilePath $Exe -PassThru
try {
    # Esperamos (hasta 15 s) a que el proceso tenga una ventana principal.
    $handle = [IntPtr]::Zero
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 300
        $proc.Refresh()
        if ($proc.MainWindowHandle -ne [IntPtr]::Zero) {
            $handle = $proc.MainWindowHandle
            break
        }
    }
    if ($handle -eq [IntPtr]::Zero) { throw "No se encontro la ventana principal de $Exe" }

    # Nos aseguramos de capturar NUESTRA ventana: la subimos al frente y la
    # ponemos como "siempre encima" mientras dura la captura.
    [void][Win32]::ShowWindow($handle, [Win32]::SW_RESTORE)
    [void][Win32]::SetWindowPos($handle, [Win32]::HWND_TOPMOST, 0, 0, 0, 0,
        [Win32]::SWP_NOMOVE -bor [Win32]::SWP_NOSIZE -bor [Win32]::SWP_SHOWWINDOW)
    [void][Win32]::SetForegroundWindow($handle)
    Start-Sleep -Seconds $WaitSeconds

    $rect = New-Object RECT
    [void][Win32]::GetWindowRect($handle, [ref]$rect)
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))

    $dir = Split-Path -Parent $Out
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)

    $g.Dispose()
    $bmp.Dispose()

    # Quitamos el "siempre encima" para no dejar la ventana molestando.
    [void][Win32]::SetWindowPos($handle, [Win32]::HWND_NOTOPMOST, 0, 0, 0, 0,
        [Win32]::SWP_NOMOVE -bor [Win32]::SWP_NOSIZE)
    Write-Output "OK: $Out ($w x $h) ventana='$([Win32]::Title($handle))'"
}
finally {
    if (-not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit() }
}
