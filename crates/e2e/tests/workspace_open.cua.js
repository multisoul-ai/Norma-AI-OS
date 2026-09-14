// L3：在 cua_repl 中先用 cua.getApp 选择已构建的隔离 App，再加载本文件。
// runWorkspaceOpenL3(app, fixturePath, logPath, screenshotPath, "canvas" | "error")
// Fixture 使用项目快照/定义的副本，清空会话、checkpoint 和派发任务，避免续跑真实任务。
// 场景：原生 Cmd+O → 目录选择 → 实际加载；通过新日志判定 Canvas/失败并保存真实截图。
// 正验收：canvas 有 workspace_open_total；error 有 failed to open workspace。
// 负验收：不得同时出现相反结果；窗口标题不能单独作为成功证据。
async function runWorkspaceOpenL3(app, fixturePath, logPath, screenshotPath, expected) {
    if (!['canvas', 'error'].includes(expected)) throw new Error('Unknown expected outcome');
    const fs = await import('node:fs/promises');
    const root = await fs.realpath(fixturePath);
    const before = await fs.readFile(logPath, 'utf8');
    await app.pressKey('super+o');
    await app.getAXState();
    await app.pressKey('super+shift+g');
    const pathState = await app.getAXState({ disableDiffing: true });
    const field = pathState.match(/^\s*(\d+) text field.*ID: PathTextField/m);
    if (!field) throw new Error('Native directory input is missing');
    await app.setValue(Number(field[1]), root);
    await app.pressKey('Return');
    const picker = await app.getAXState({ disableDiffing: true });
    const open = picker.match(/^\s*(\d+) button Open Project, ID: OKButton/m);
    if (!open) throw new Error('Enabled Open Project button is missing');
    await app.click(Number(open[1]));
    const result = await app.getAXStateAndScreenshot();
    if (!result.screenshot) throw new Error('Real app screenshot is missing');
    await fs.writeFile(screenshotPath, result.screenshot);
    const lines = (await fs.readFile(logPath, 'utf8')).slice(before.length).split('\n')
        .filter(line => line.includes(`path=${root} `) || line.endsWith(`path=${root}`));
    const entered = lines.some(line => line.includes('stage="workspace_open_total"'));
    const failed = lines.some(line => line.includes('failed to open workspace '));
    if (entered !== (expected === 'canvas') || failed !== (expected === 'error')) {
        throw new Error(`Unexpected project open outcome: entered=${entered}, failed=${failed}`);
    }
    return { expected, screenshotPath };
}
