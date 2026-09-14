param(
    [string]$Out = "shot_win.png",
    [int]$MaxWaitSec = 20
)
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Rect {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
$handle = [IntPtr]::Zero
$rect = New-Object Win32Rect+RECT
for ($i = 0; $i -lt $MaxWaitSec * 2; $i++) {
    $proc = Get-Process fuck_ensp -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if ($proc) {
        $h = $proc.MainWindowHandle
        if ([Win32Rect]::IsWindowVisible($h) -and [Win32Rect]::GetWindowRect($h, [ref]$rect) -and ($rect.Right - $rect.Left) -gt 0) {
            $handle = $h
            break
        }
    }
    Start-Sleep -Milliseconds 500
}
if ($handle -eq [IntPtr]::Zero) { Write-Error "no visible window"; exit 1 }
[void][Win32Rect]::SetForegroundWindow($handle)
Start-Sleep -Milliseconds 600
[void][Win32Rect]::GetWindowRect($handle, [ref]$rect)
$w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
Write-Output ("window: x={0} y={1} w={2} h={3}" -f $rect.Left, $rect.Top, $w, $h)
$b = [System.Windows.Forms.SystemInformation]::PrimaryMonitorSize
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
[System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen(0, 0, 0, 0, $bmp.Size)
$x = [Math]::Max(0, $rect.Left); $y = [Math]::Max(0, $rect.Top)
$cw = [Math]::Min($b.Width - $x, $w); $ch = [Math]::Min($b.Height - $y, $h)
$crop = $bmp.Clone((New-Object System.Drawing.Rectangle $x, $y, $cw, $ch), $bmp.PixelFormat)
$crop.Save($Out)
Write-Output "saved $Out"
