# 场景：真实App使用stderr日志；就绪门槛不能因读取stdout而把已初始化误判为失败。
# 构造：独立临时目录中的两个UTF-8日志，分别放空内容、真实ready事件和仅排队事件。
# 执行：用生产Test-NormaNativeReady检查stderr-only、stdout-only和两个负例。
# 预期：两种日志流都接受真实ready；空日志和creation_requested均不能假绿。
$ErrorActionPreference='Stop'
. "$PSScriptRoot/desktop-driver.ps1"
# The package wrapper loads the same driver for smoke and then workspace acceptance.
. "$PSScriptRoot/desktop-driver.ps1"
$root=Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory $root | Out-Null
try {
    Set-Content (Join-Path $root 'stdout.log') '' -Encoding utf8
    Set-Content (Join-Path $root 'stderr.log') '' -Encoding utf8
    if (Test-NormaNativeReady $root) { throw 'Empty logs must not pass native creation' }
    Set-Content (Join-Path $root 'stderr.log') 'INFO app::browser::deferred: native_webview_ready node_id=None' -Encoding utf8
    if (-not (Test-NormaNativeReady $root)) { throw 'Actual stderr readiness must be recognized' }
    Set-Content (Join-Path $root 'stderr.log') '' -Encoding utf8
    Set-Content (Join-Path $root 'stdout.log') 'INFO app::browser::deferred: native_webview_ready node_id=None' -Encoding utf8
    if (-not (Test-NormaNativeReady $root)) { throw 'Stdout readiness must remain accepted' }
    Set-Content (Join-Path $root 'stdout.log') 'INFO browser_view_creation_requested' -Encoding utf8
    if (Test-NormaNativeReady $root) { throw 'Queued placeholder must not count as native creation' }
    Write-Output 'PASS: four native readiness log contracts'

    # 场景：首次包启动有ERROR但退出0，后续启动干净，不能因此获得同包验收PASS。
    # 构造：真实双流日志文件；干净INFO/WARN、ANSI Rust ERROR、Chromium ERROR和panic。
    # 执行：调用共用App日志检查，分别向stdout/stderr写入每个反例。
    # 预期：干净日志通过；任意流的三类错误均拒绝，普通WARN不能误判。
    Set-Content (Join-Path $root 'stdout.log') 'INFO ready' -Encoding utf8
    Set-Content (Join-Path $root 'stderr.log') 'WARN optional feature unavailable' -Encoding utf8
    Assert-NormaAppLogs $root
    foreach ($stream in @('stdout.log','stderr.log')) {
        foreach ($line in @("2026-11-03T00:00:00Z $([char]27)[31mERROR$([char]27)[0m native startup", '[123:456:ERROR:browser.cc:1] native failure', "thread 'main' panicked at startup.rs:1")) {
            Set-Content (Join-Path $root $stream) $line -Encoding utf8
            $rejected=$false
            try { Assert-NormaAppLogs $root }
            catch { if ($_.Exception.Message -notlike '*Actual App emitted an error*') { throw }; $rejected=$true }
            if (-not $rejected) { throw "App errors must fail acceptance even when process exit is zero: $stream / $line" }
            Set-Content (Join-Path $root $stream) 'INFO ready' -Encoding utf8
        }
    }
    Write-Output 'PASS: clean App logs and six zero-exit error rejection contracts'

    # 场景：截图必须只覆盖真实窗口client，不能夹带边框、桌面或任务栏。
    # 构造：640×360的真实WinForms client；随后将同一窗口移出可见桌面。
    # 执行：走生产Save-NormaScreenshot并读PNG尺寸，再检查不可见窗口明确失败。
    # 预期：宽高精确640×360；越界不得生成可冒充产品证据的黑屏PNG。
    Add-Type -AssemblyName System.Windows.Forms
    $form=[System.Windows.Forms.Form]::new()
    try {
        $form.StartPosition=[System.Windows.Forms.FormStartPosition]::Manual
        $form.Location=[System.Drawing.Point]::new(32,32)
        $form.ClientSize=[System.Drawing.Size]::new(640,360)
        $form.Show()
        [System.Windows.Forms.Application]::DoEvents()
        $png=Join-Path $root 'client.png'
        Save-NormaScreenshot $form.Handle $png
        $bitmap=[System.Drawing.Bitmap]::new($png)
        try {
            if ($bitmap.Width -ne 640 -or $bitmap.Height -ne 360) {
                throw "Capture must contain only 640x360 client pixels; got $($bitmap.Width)x$($bitmap.Height)"
            }
        } finally { $bitmap.Dispose() }
        $form.Location=[System.Drawing.Point]::new(-2000,0)
        [System.Windows.Forms.Application]::DoEvents()
        $rejected=$false
        try { Save-NormaScreenshot $form.Handle (Join-Path $root 'offscreen.png') }
        catch { if ($_.Exception.Message -notlike '*not wholly visible*') { throw }; $rejected=$true }
        if (-not $rejected) { throw 'Offscreen client must be rejected as visual evidence' }
        Write-Output 'PASS: native client capture and offscreen rejection'
    } finally { $form.Close(); $form.Dispose() }
} finally { Remove-Item $root -Recurse -Force }
