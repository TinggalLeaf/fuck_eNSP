param(
    [int]$X = 0,
    [int]$Y = 0,
    [string]$Out = "shot.png",
    [switch]$Click
)
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
if ($Click) {
    $proc = Get-Process fuck_ensp -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if ($proc) {
        $sig2 = '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool SetForegroundWindow(System.IntPtr h);'
        Add-Type -MemberDefinition $sig2 -Name F -Namespace W
        [void][W.F]::SetForegroundWindow($proc.MainWindowHandle)
        Start-Sleep -Milliseconds 500
    }
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point($X, $Y)
    Start-Sleep -Milliseconds 150
    $sig = '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern void mouse_event(int f, int dx, int dy, int d, int i);'
    Add-Type -MemberDefinition $sig -Name U -Namespace W
    [W.U]::mouse_event(0x0002, 0, 0, 0, 0)
    Start-Sleep -Milliseconds 120
    [W.U]::mouse_event(0x0004, 0, 0, 0, 0)
    Start-Sleep -Milliseconds 800
}
$b = [System.Windows.Forms.SystemInformation]::PrimaryMonitorSize
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
[System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen(0, 0, 0, 0, $bmp.Size)
$bmp.Save($Out)
