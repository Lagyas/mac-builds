# List every ComboBox (and its items) under the windows of a process.
param([int]$ProcId)
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public class C {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Ansi)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, StringBuilder l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
  public static string Txt(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
  public static List<string> Dump(uint pid) {
    var o = new List<string>();
    EnumWindows((t, x) => { uint p; GetWindowThreadProcessId(t, out p); if (p != pid) return true;
      EnumChildWindows(t, (h, y) => {
        if (Cls(h) == "ComboBox") {
          int n = (int)SendMessage(h, 0x146, IntPtr.Zero, IntPtr.Zero); // CB_GETCOUNT
          var items = new List<string>();
          for (int i = 0; i < n && i < 400; i++) { var sb = new StringBuilder(256); SendMessage(h, 0x148, (IntPtr)i, sb); items.Add(sb.ToString()); }
          IntPtr par = GetParent(h);
          o.Add(string.Format("[{0}] parent='{1}'/{2} n={3}: {4}", h, Txt(par), Cls(par), n, string.Join(" | ", items)));
        }
        return true; }, IntPtr.Zero);
      return true; }, IntPtr.Zero);
    return o;
  }
}
"@
[C]::Dump([uint32]$ProcId)
