# Move the mouse to window-relative (x,y) points and save the cursor shown there.
# usage: cursor.ps1 -Hwnd 123 -Points "10,20;30,40" -Out F:\rea\my_shots\cur  (-> cur_0.png, cur_1.png …)
param([int64]$Hwnd, [string]$Points, [string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class CU {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct CURSORINFO { public int cbSize; public int flags; public IntPtr hCursor; public POINT pt; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorInfo(ref CURSORINFO ci);
  [DllImport("user32.dll")] public static extern bool DrawIconEx(IntPtr hdc, int x, int y, IntPtr icon, int w, int h, int step, IntPtr brush, int flags);
}
"@
$r = New-Object CU+RECT; [void][CU]::GetWindowRect([IntPtr]$Hwnd, [ref]$r)
$k = 0
foreach ($p in $Points.Split(';')) {
  $xy = $p.Split(','); [void][CU]::SetCursorPos($r.L + [int]$xy[0], $r.T + [int]$xy[1])
  Start-Sleep -Milliseconds 120; [void][CU]::SetCursorPos($r.L + [int]$xy[0] + 1, $r.T + [int]$xy[1]); Start-Sleep -Milliseconds 350
  $ci = New-Object CU+CURSORINFO; $ci.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($ci); [void][CU]::GetCursorInfo([ref]$ci)
  $bmp = New-Object System.Drawing.Bitmap 32, 32; $g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::FromArgb(200, 200, 200))
  $hdc = $g.GetHdc(); [void][CU]::DrawIconEx($hdc, 0, 0, $ci.hCursor, 32, 32, 0, [IntPtr]::Zero, 3); $g.ReleaseHdc($hdc); $g.Dispose()
  $f = "{0}_{1}.png" -f $Out, $k; $bmp.Save($f); $bmp.Dispose(); "$p -> $f (h=$($ci.hCursor))"; $k++
}
