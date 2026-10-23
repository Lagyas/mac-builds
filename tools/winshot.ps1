# Capture a window's screen area including popups/menus over it.  usage: winshot.ps1 -Hwnd h -Out file.png [-Pad 0]
param([int64]$Hwnd, [string]$Out, [int]$Pad = 0)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class WRS { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r); public struct R { public int L,T,Ri,B; } }
"@
$r = New-Object WRS+R
[WRS]::GetWindowRect([IntPtr]$Hwnd, [ref]$r) | Out-Null
$x = [Math]::Max(0, $r.L - $Pad); $y = [Math]::Max(0, $r.T - $Pad)
$w = $r.Ri - $r.L + 2 * $Pad; $h = $r.B - $r.T + 2 * $Pad
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ${w}x${h} at $($r.L),$($r.T)"
