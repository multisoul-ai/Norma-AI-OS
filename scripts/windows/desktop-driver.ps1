# Native input only: this driver never calls App internals or writes its database.
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NormaDesktop {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Keyboard { public ushort Key, Scan; public uint Flags, Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Explicit, Size=32)] public struct InputData { [FieldOffset(0)] public Keyboard Keyboard; }
    [StructLayout(LayoutKind.Sequential)] public struct Input { public uint Type; public InputData Data; }
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr first, IntPtr second);
    public static void Key(ushort key, bool up) { Send(key, 0, up ? 2u : 0u); }
    public static void Text(string text) {
        foreach (char c in text) { Send(0, c, 4); Send(0, c, 6); }
    }
    static void Send(ushort key, ushort scan, uint flags) {
        var input = new Input { Type=1, Data=new InputData { Keyboard=new Keyboard { Key=key, Scan=scan, Flags=flags } } };
        if (SendInput(1, new [] { input }, Marshal.SizeOf(typeof(Input))) != 1) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    }
}
'@
function Get-NormaWindow($Process) {
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while ([DateTime]::UtcNow -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) { throw "App exited before window creation: $($Process.ExitCode)" }
        if ($Process.MainWindowHandle -ne [IntPtr]::Zero) { return $Process.MainWindowHandle }
        Start-Sleep -Milliseconds 250
    }
    throw 'No visible App window'
}
function Focus-NormaWindow([IntPtr]$Window) {
    [NormaDesktop]::ShowWindow($Window, 9) | Out-Null
    [NormaDesktop]::SetForegroundWindow($Window) | Out-Null
    Start-Sleep -Milliseconds 300
    if ([NormaDesktop]::GetForegroundWindow() -ne $Window) { throw 'Input target is not the foreground App window' }
}
function Send-NormaChord([ushort[]]$Keys) {
    foreach ($key in $Keys) { [NormaDesktop]::Key($key, $false) }
    for ($i=$Keys.Count-1; $i -ge 0; $i--) { [NormaDesktop]::Key($Keys[$i], $true) }
}
function Save-NormaScreenshot([IntPtr]$Window, [string]$Path) {
    $rect = [NormaDesktop+Rect]::new()
    if (-not [NormaDesktop]::GetWindowRect($Window, [ref]$rect)) { throw 'No window bounds' }
    $width=$rect.Right-$rect.Left; $height=$rect.Bottom-$rect.Top
    if ($width -lt 300 -or $height -lt 200) { throw 'Window has invalid dimensions' }
    $bitmap=[System.Drawing.Bitmap]::new($width,$height)
    $graphics=[System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size)
        $bitmap.Save($Path,[System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Close-NormaWindow($Process, [IntPtr]$Window) {
    if (-not [NormaDesktop]::PostMessage($Window,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)) { throw 'WM_CLOSE failed' }
    if (-not $Process.WaitForExit(10000)) { throw 'Last-window close did not exit App' }
    if ($Process.ExitCode -ne 0) { throw "App exit code $($Process.ExitCode)" }
}
