param([string]$Out = "shot_pw.png")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class PW {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleDC(IntPtr hdc);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleBitmap(IntPtr hdc, int w, int h);
    [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr hdc, IntPtr obj);
    [DllImport("gdi32.dll")] public static extern bool DeleteDC(IntPtr hdc);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr obj);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr h);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
$proc = Get-Process fuck_ensp -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Error "no window"; exit 1 }
$h = $proc.MainWindowHandle
$r = New-Object PW+RECT
[void][PW]::GetWindowRect($h, [ref]$r)
$w = $r.Right - $r.Left; $hgt = $r.Bottom - $r.Top
Write-Output ("window: {0}x{1} at {2},{3}" -f $w, $hgt, $r.Left, $r.Top)
$winDC = [PW]::GetWindowDC($h)
$memDC = [PW]::CreateCompatibleDC($winDC)
$bmpHandle = [PW]::CreateCompatibleBitmap($winDC, $w, $hgt)
[void][PW]::SelectObject($memDC, $bmpHandle)
# PW_RENDERFULLCONTENT = 2
[void][PW]::PrintWindow($h, $memDC, 2)
$bmp = [System.Drawing.Image]::FromHbitmap($bmpHandle)
$bmp.Save($Out)
[void][PW]::DeleteObject($bmpHandle)
[void][PW]::DeleteDC($memDC)
Write-Output "saved $Out"
