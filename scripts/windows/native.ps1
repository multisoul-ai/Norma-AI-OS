param(
    [ValidateSet('environment', 'graphics')][string]$Mode,
    [Parameter(Mandatory)][string]$Output,
    [string]$Executable
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Output | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

function Get-ProbePixelChannel([System.Drawing.Color]$Pixel) {
    if ($Pixel.R -gt 170 -and $Pixel.G -lt 100 -and $Pixel.B -lt 100) { return 'red' }
    # GPUI 0.2.2 green() is CSS green (0,128,0), not lime (0,255,0).
    if ($Pixel.G -gt 100 -and $Pixel.R -lt 100 -and $Pixel.B -lt 100) { return 'green' }
    if ($Pixel.B -gt 170 -and $Pixel.R -lt 100 -and $Pixel.G -lt 100) { return 'blue' }
    return ''
}

# 场景：固定 RGB 色样与灰背景；执行同一像素分类接口；预期 CSS green 被识别，灰背景不算颜色。
$colorCases = @(
    @{ pixel = [System.Drawing.Color]::FromArgb(255, 0, 0); expected = 'red' },
    @{ pixel = [System.Drawing.Color]::FromArgb(0, 128, 0); expected = 'green' },
    @{ pixel = [System.Drawing.Color]::FromArgb(0, 0, 255); expected = 'blue' },
    @{ pixel = [System.Drawing.Color]::FromArgb(80, 80, 80); expected = '' },
    @{ pixel = [System.Drawing.Color]::FromArgb(0, 0, 0); expected = '' }
)
foreach ($case in $colorCases) {
    if ((Get-ProbePixelChannel $case.pixel) -ne $case.expected) {
        throw "Fixed-color classifier self-test failed: $($case.expected)"
    }
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace NormaProbe {
    public static class Native {
        [DllImport("d3d11.dll")] static extern int D3D11CreateDevice(
            IntPtr adapter, int driver, IntPtr software, uint flags, IntPtr levels,
            uint count, uint sdk, out IntPtr device, out uint level, out IntPtr context);
        public static string DirectX(int driver) {
            IntPtr device, context; uint level;
            int hr = D3D11CreateDevice(IntPtr.Zero, driver, IntPtr.Zero, 0x20,
                IntPtr.Zero, 0, 7, out device, out level, out context);
            if (context != IntPtr.Zero) Marshal.Release(context);
            if (device != IntPtr.Zero) Marshal.Release(device);
            return String.Format("HRESULT=0x{0:X8};feature_level=0x{1:X}", hr, level);
        }
        public delegate bool EnumProc(IntPtr window, IntPtr extra);
        [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr extra);
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
        [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr window);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int w, int h, uint flags);
        [DllImport("user32.dll")] static extern bool AttachThreadInput(uint first, uint second, bool attach);
        [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
        [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int w, int h, bool repaint);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("kernel32.dll")] public static extern uint WTSGetActiveConsoleSessionId();
        [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        public static string ClassName(IntPtr window) {
            var text = new StringBuilder(256); GetClassName(window, text, text.Capacity); return text.ToString();
        }
        public static string Describe(IntPtr window) {
            uint pid; GetWindowThreadProcessId(window, out pid);
            var text = new StringBuilder(512); GetWindowText(window, text, text.Capacity);
            return String.Format("hwnd={0};pid={1};class={2};title={3}", window, pid, ClassName(window), text);
        }
        public static string[] VisibleWindows(int process) {
            var windows = new List<string>();
            EnumWindows((window, extra) => {
                uint pid; GetWindowThreadProcessId(window, out pid);
                if (pid == process && IsWindowVisible(window)) windows.Add(Describe(window));
                return true;
            }, IntPtr.Zero);
            return windows.ToArray();
        }
        public static string Activate(IntPtr window) {
            uint pid;
            uint other = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
            uint own = GetCurrentThreadId();
            bool attached = other != 0 && other != own && AttachThreadInput(own, other, true);
            try {
                ShowWindow(window, 9); BringWindowToTop(window);
                bool activated = SetForegroundWindow(window);
                return String.Format("attached={0};activated={1};foreground={2}", attached, activated, Describe(GetForegroundWindow()));
            } finally { if (attached) AttachThreadInput(own, other, false); }
        }
        public static IntPtr FindWindow(int process) {
            IntPtr found = IntPtr.Zero;
            EnumWindows((window, extra) => {
                uint pid; GetWindowThreadProcessId(window, out pid);
                if (pid == process && IsWindowVisible(window) && ClassName(window) == "Zed::Window") {
                    found = window; return false;
                }
                return true;
            }, IntPtr.Zero);
            return found;
        }
        public static void CloseByKeyboard() {
            keybd_event(0x12, 0, 0, UIntPtr.Zero);
            keybd_event(0x73, 0, 0, UIntPtr.Zero);
            keybd_event(0x73, 0, 2, UIntPtr.Zero);
            keybd_event(0x12, 0, 2, UIntPtr.Zero);
        }
    }
}
'@

$checks = [System.Collections.Generic.List[object]]::new()
$data = [ordered]@{
    mode = $Mode
    os = @(Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, OSArchitecture)
    image_os = $env:ImageOS
    image_version = $env:ImageVersion
    user_interactive = [Environment]::UserInteractive
    process_session = (Get-Process -Id $PID).SessionId
    active_console_session = [NormaProbe.Native]::WTSGetActiveConsoleSessionId()
    displays = @([System.Windows.Forms.Screen]::AllScreens | ForEach-Object {
        @{ device = $_.DeviceName; primary = $_.Primary; bounds = $_.Bounds.ToString() }
    })
    adapters = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
    directx_hardware = [NormaProbe.Native]::DirectX(1)
    directx_warp = [NormaProbe.Native]::DirectX(5)
    pixel_classifier_self_test = 'PASS'
}
$checks.Add(@{ id = 'native_environment'; status = 'PASS'; detail = 'OS, displays and D3D11 device creation were queried' })
foreach ($kind in @('hardware', 'warp')) {
    $value = $data["directx_$kind"]
    $checks.Add(@{ id = "d3d11_$kind"; status = $(if ($value.StartsWith('HRESULT=0x00000000')) { 'PASS' } else { 'ENV_UNSUPPORTED' }); detail = $value })
}

if ($Mode -eq 'graphics') {
    $process = $null
    $failureStatus = 'FAIL'
    try {
        if ([System.Windows.Forms.Screen]::AllScreens.Count -eq 0) {
            $failureStatus = 'ENV_UNSUPPORTED'
            throw 'The runner exposes no display'
        }
        $process = Start-Process -FilePath $Executable -PassThru `
            -RedirectStandardOutput (Join-Path $Output 'gpui.stdout.log') `
            -RedirectStandardError (Join-Path $Output 'gpui.stderr.log')
        $deadline = [DateTime]::UtcNow.AddSeconds(35)
        $window = [IntPtr]::Zero
        while ([DateTime]::UtcNow -lt $deadline) {
            $process.Refresh()
            if ($process.HasExited) { throw "GPUI exited before window discovery: $($process.ExitCode)" }
            $window = [NormaProbe.Native]::FindWindow($process.Id)
            if ($window -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 250
        }
        if ($window -eq [IntPtr]::Zero) { throw 'No visible top-level GPUI window within 35 seconds' }
        $data['visible_windows'] = @([NormaProbe.Native]::VisibleWindows($process.Id))
        $data['foreground_before'] = [NormaProbe.Native]::Describe([NormaProbe.Native]::GetForegroundWindow())
        [void][NormaProbe.Native]::MoveWindow($window, 60, 60, 640, 560, $true)
        [void][NormaProbe.Native]::SetWindowPos($window, [IntPtr]::new(-1), 0, 0, 0, 0, 0x13)
        $data['activation'] = [NormaProbe.Native]::Activate($window)
        Start-Sleep -Seconds 2
        $rect = [NormaProbe.Native+Rect]::new()
        if (-not [NormaProbe.Native]::GetWindowRect($window, [ref]$rect)) { throw 'GetWindowRect failed' }
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        if ($width -lt 100 -or $height -lt 100) { throw 'Window rectangle is too small' }
        $bitmap = [System.Drawing.Bitmap]::new($width, $height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $bitmap.Save((Join-Path $Output 'gpui-window.png'), [System.Drawing.Imaging.ImageFormat]::Png)
            $samples = @{ red = 0; green = 0; blue = 0 }
            for ($y = 0; $y -lt $height; $y += 3) {
                for ($x = 0; $x -lt $width; $x += 3) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    $channel = Get-ProbePixelChannel $pixel
                    if ($channel) { $samples[$channel]++ }
                }
            }
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        $data['window'] = @{ pid = $process.Id; width = $width; height = $height; foreground = ([NormaProbe.Native]::GetForegroundWindow() -eq $window) }
        $data['color_samples'] = $samples
        if ($samples.red -lt 20 -or $samples.green -lt 20 -or $samples.blue -lt 20) {
            throw 'Expected red/green/blue GPUI sample pixels were not rendered'
        }
        $checks.Add(@{ id = 'gpui_window'; status = 'PASS'; detail = 'Owned Zed::Window and expected GPUI RGB pixels captured'; screenshot = 'gpui-window.png' })
        if ([NormaProbe.Native]::GetForegroundWindow() -ne $window) {
            $failureStatus = 'ENV_UNSUPPORTED'
            throw 'Foreground ownership changed after capture; no keyboard event sent'
        }
        [NormaProbe.Native]::CloseByKeyboard()
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while ([DateTime]::UtcNow -lt $deadline -and [NormaProbe.Native]::IsWindow($window)) {
            Start-Sleep -Milliseconds 100
        }
        $closed = -not [NormaProbe.Native]::IsWindow($window)
        $checks.Add(@{ id = 'gpui_os_keyboard_close'; status = $(if ($closed) { 'PASS' } else { 'FAIL' }); detail = 'Alt+F4 through the OS keyboard path; only the owned foreground sample was targeted' })
    } catch {
        $failedId = if (@($checks | Where-Object { $_.id -eq 'gpui_window' }).Count -gt 0) { 'gpui_os_keyboard_close' } else { 'gpui_window' }
        $checks.Add(@{ id = $failedId; status = $failureStatus; detail = ($_ | Out-String) })
        $data['foreground_after'] = [NormaProbe.Native]::Describe([NormaProbe.Native]::GetForegroundWindow())
        if ($null -ne $process) { $data['visible_windows'] = @([NormaProbe.Native]::VisibleWindows($process.Id)) }
        try {
            $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
            $snapshot = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
            $capture = [System.Drawing.Graphics]::FromImage($snapshot)
            try {
                $capture.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $snapshot.Size)
                $snapshot.Save((Join-Path $Output 'desktop-diagnostic.png'), [System.Drawing.Imaging.ImageFormat]::Png)
            } finally { $capture.Dispose(); $snapshot.Dispose() }
        } catch { $data['diagnostic_screenshot_error'] = ($_ | Out-String) }
    } finally {
        if ($null -ne $process -and -not $process.HasExited) {
            $process.Kill($true)
            [void]$process.WaitForExit(10000)
        }
    }
}
$data['checks'] = @($checks.ToArray())
$data | ConvertTo-Json -Depth 8 | Set-Content -Path (Join-Path $Output 'native.json') -Encoding utf8
