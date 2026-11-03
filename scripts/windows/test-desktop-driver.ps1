# 场景：真实App使用stderr日志；就绪门槛不能因读取stdout而把已初始化误判为失败。
# 构造：独立临时目录中的两个UTF-8日志，分别放空内容、真实ready事件和仅排队事件。
# 执行：用生产Test-NormaNativeReady检查stderr-only、stdout-only和两个负例。
# 预期：两种日志流都接受真实ready；空日志和creation_requested均不能假绿。
$ErrorActionPreference='Stop'
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
} finally { Remove-Item $root -Recurse -Force }
