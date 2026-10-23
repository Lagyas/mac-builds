# Drive the mouse/keyboard inside a window (coordinates relative to the window's top-left).
# mouse.ps1 -Hwnd <h> -Drag "x1,y1,x2,y2" | -Click "x,y" | -RClick "x,y" | -DblClick "x,y" | -Keys "^c" (SendKeys syntax)
param([int64]$Hwnd, [string]$Drag = "", [string]$Click = "", [string]$Keys = "", [string]$RClick = "", [string]$DblClick = "")
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Threading;
public class MZ {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Ri, B; }
  public static void Drag(IntPtr h, int x1, int y1, int x2, int y2) {
    R r; GetWindowRect(h, out r); SetForegroundWindow(h); Thread.Sleep(250);
    SetCursorPos(r.L + x1, r.T + y1); Thread.Sleep(80);
    mouse_event(2, 0, 0, 0, IntPtr.Zero); Thread.Sleep(80);
    for (int i = 1; i <= 20; i++) { SetCursorPos(r.L + x1 + (x2 - x1) * i / 20, r.T + y1 + (y2 - y1) * i / 20); Thread.Sleep(25); }
    Thread.Sleep(80); mouse_event(4, 0, 0, 0, IntPtr.Zero); Thread.Sleep(200);
  }
  public static void Btn(IntPtr h, int x, int y, uint down, uint up, int times) {
    R r; GetWindowRect(h, out r); SetForegroundWindow(h); Thread.Sleep(250);
    SetCursorPos(r.L + x, r.T + y); Thread.Sleep(80);
    for (int i = 0; i < times; i++) { mouse_event(down, 0, 0, 0, IntPtr.Zero); Thread.Sleep(40); mouse_event(up, 0, 0, 0, IntPtr.Zero); Thread.Sleep(60); }
    Thread.Sleep(200);
  }
  public static void Click(IntPtr h, int x, int y) {
    R r; GetWindowRect(h, out r); SetForegroundWindow(h); Thread.Sleep(250);
    SetCursorPos(r.L + x, r.T + y); Thread.Sleep(80);
    mouse_event(2, 0, 0, 0, IntPtr.Zero); Thread.Sleep(60); mouse_event(4, 0, 0, 0, IntPtr.Zero); Thread.Sleep(200);
  }
}
"@
$h = [IntPtr]$Hwnd
if ($Drag) { $v = $Drag.Split(',') | ForEach-Object { [int]$_ }; [MZ]::Drag($h, $v[0], $v[1], $v[2], $v[3]); "dragged" }
if ($Click) { $v = $Click.Split(',') | ForEach-Object { [int]$_ }; [MZ]::Click($h, $v[0], $v[1]); "clicked" }
if ($RClick) { $v = $RClick.Split(',') | ForEach-Object { [int]$_ }; [MZ]::Btn($h, $v[0], $v[1], 8, 16, 1); "rclicked" }
if ($DblClick) { $v = $DblClick.Split(',') | ForEach-Object { [int]$_ }; [MZ]::Btn($h, $v[0], $v[1], 2, 4, 2); "dblclicked" }
if ($Keys) { [void][MZ]::SetForegroundWindow($h); Start-Sleep -Milliseconds 250; Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait($Keys); Start-Sleep -Milliseconds 300; "keys $Keys" }
