# MDI control of a child window: mdi.ps1 -Child <hwnd> -Action activate|maximize|restore
param([int64]$Child, [string]$Action)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class M {
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
}
"@
$c = [IntPtr]$Child; $client = [M]::GetParent($c)
switch ($Action) {
  "activate" { [void][M]::SendMessage($client, 0x222, $c, [IntPtr]::Zero) }  # WM_MDIACTIVATE
  "maximize" { [void][M]::SendMessage($client, 0x225, $c, [IntPtr]::Zero) }  # WM_MDIMAXIMIZE
  "restore"  { [void][M]::SendMessage($client, 0x223, $c, [IntPtr]::Zero) }  # WM_MDIRESTORE
}
"ok $Action"
