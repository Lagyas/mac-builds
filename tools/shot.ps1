# Capture a window of a process to PNG (works when the window is covered).
# usage: shot.ps1 -ProcId 1700 -Out F:\rea\orig_shots\main.png [-TitleLike "Piano*"]
param([int]$ProcId, [string]$Out, [string]$TitleLike = "*")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static List<IntPtr> Top(uint pid) {
    var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) l.Add(h); return true; }, IntPtr.Zero);
    return l;
  }
  public static List<IntPtr> All(uint pid) {
    var l = new List<IntPtr>();
    foreach (var t in Top(pid)) { l.Add(t); EnumChildWindows(t, (h, x) => { if (IsWindowVisible(h)) l.Add(h); return true; }, IntPtr.Zero); }
    return l;
  }
  public static string Text(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
}
"@
$wins = [W]::All([uint32]$ProcId)
foreach ($h in ($wins | Where-Object { [W]::Text($_) -ne "" })) { $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r); "{0}  '{1}'  {2}x{3}" -f $h, [W]::Text($h), ($r.R-$r.L), ($r.B-$r.T) }
$h = $wins | Where-Object { [W]::Text($_) -like $TitleLike } | Sort-Object { $r = New-Object W+RECT; [void][W]::GetWindowRect($_, [ref]$r); -1 * ($r.R-$r.L) * ($r.B-$r.T) } | Select-Object -First 1
if (-not $h) { "no window"; exit 1 }
if ([W]::IsIconic($h)) { [void][W]::ShowWindow($h, 9); Start-Sleep -Milliseconds 700 }
$r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r)
$bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
[void][W]::PrintWindow($h, $hdc, 2); $g.ReleaseHdc($hdc); $g.Dispose()
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
"saved $Out"
