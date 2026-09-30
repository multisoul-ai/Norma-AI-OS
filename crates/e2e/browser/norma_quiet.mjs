/**
 * 场景：默认画布通过逐步展开操作，保留 CLI、画笔与团队协作闭环。
 * 数据：初始四个内容节点、一条真实指针笔迹、模拟 CLI 指令。
 * 执行：用实际控件输入、选择、绘制、撤销、打开团队与内容菜单。
 * 预期：默认四个画布入口，相关操作只在需要时出现，工具条不遮挡对话。
 * 同时兼容 CUA tab.playwright 与 Playwright Page；evaluate 仅检查渲染后的 DOM。
 */
export async function runNormaQuietE2E(page, pointer) {
  const results = [];
  const ensure = (condition, message) => {
    if (!condition) throw new Error(message);
    results.push(message);
  };
  const button = (name) => page.getByRole("button", { name, exact: true });
  const count = async () =>
    Number((await page.locator(".quiet-canvas-state").innerText()).match(/(\d+)\s*处标注/)?.[1]);
  ensure(
    await page.getByLabel("给 Codex 的 CLI 指令", { exact: true }).isVisible(),
    "默认内容包括 Codex CLI",
  );
  ensure(
    await page.getByLabel("给 Claude 的 CLI 指令", { exact: true }).isVisible(),
    "默认内容包括 Claude CLI",
  );
  ensure(
    (await page.locator(".quiet-canvas-ornament > button").count()) === 4,
    "默认工具条只有四个入口",
  );
  ensure(!(await page.getByLabel("橡皮", { exact: true }).isVisible()), "未画图时隐藏橡皮");
  ensure(
    !(await page.getByRole("region", { name: "团队面板", exact: true }).isVisible()),
    "默认不显示团队操作面板",
  );
  ensure(
    await page.evaluate(
      () =>
        document.querySelector(".quiet-canvas-ornament").getBoundingClientRect().bottom <
        document.querySelector(".quiet-conversation").getBoundingClientRect().top,
    ),
    "工具条与对话区域不重叠",
  );
  await page.getByLabel("给 Codex 的 CLI 指令", { exact: true }).fill("检查输入焦点路由");
  await page.getByLabel("给 Codex 的 CLI 指令", { exact: true }).press("Enter");
  ensure(
    (await page.getByRole("log", { name: "Codex 终端输出", exact: true }).innerText()).includes(
      "检查输入焦点路由",
    ),
    "精简界面仍可直接启动 CLI",
  );
  await button("Codex · 实现 操作").click();
  ensure(await button("中断会话").isVisible(), "选中 CLI 后提供会话操作");
  await button("中断会话").click();
  ensure(
    (await page.locator(".node-codex .cli-session-state").innerText()).includes("已中断"),
    "上下文操作可以中断 CLI",
  );
  await button("查看团队").click();
  ensure((await page.locator(".quiet-agent").count()) === 3, "团队入口展开三个 Agent");
  await button("查看团队").click();
  await page.getByLabel("画笔", { exact: true }).click();
  ensure(await page.getByLabel("橡皮", { exact: true }).isVisible(), "进入绘图才展开橡皮");
  const before = await count();
  const point = await page.evaluate(() => {
    const r = document.querySelector(".node-codex").getBoundingClientRect();
    return [r.x + 45, r.y + 120];
  });
  await pointer.drag(point, [point[0] + 150, point[1] + 20]);
  ensure((await count()) === before + 1, "精简画布可真实绘制笔迹");
  await button("撤销笔迹").click();
  ensure((await count()) === before, "上下文撤销可用");
  await button("重做笔迹").click();
  ensure((await count()) === before + 1, "上下文重做可用");
  await button("标注交给 Norma").click();
  ensure(
    (await page.getByRole("log", { name: "Codex 终端输出", exact: true }).innerText()).includes(
      "画笔标注调整，关联内容：Codex",
    ),
    "笔迹连同 CLI 上下文交给 Norma",
  );
  ensure(!(await page.getByLabel("橡皮", { exact: true }).isVisible()), "提交后自动收起绘图操作");
  await button("空间选项").click();
  ensure(await button("空间漫游").isVisible(), "空间模式在设置入口按需呈现");
  await button("空间选项").click();
  await page.getByLabel("给 Codex 的 CLI 指令", { exact: true }).click();
  await button("Codex · 实现 操作").click();
  await button("专注阅读").click();
  ensure(
    await page.getByRole("region", { name: "内容阅读", exact: true }).isVisible(),
    "CLI 可进入专注阅读",
  );
  ensure(!(await page.locator(".quiet-canvas").isVisible()), "专注时隐藏画布并隔离其输入");
  await button("返回自由画布").click();
  ensure(
    await page.evaluate(
      () => document.querySelector(".node-codex").getBoundingClientRect().width > 200,
    ),
    "返回画布后内容保持正常可读尺寸",
  );
  return results;
}

/** 场景：用户打开默认 VR 原型。执行：检查真实渲染区域与入口；预期没有二维编辑控件混入空间。 */
export async function runNormaVRPreviewE2E(page) {
  const results = [];
  const ensure = (condition, message) => {
    if (!condition) throw new Error(message);
    results.push(message);
  };
  ensure(
    await page.getByRole("region", { name: "三维空间预览", exact: true }).isVisible(),
    "主体验为实际三维空间",
  );
  ensure(
    await page.getByRole("button", { name: "进入 VR", exact: true }).isVisible(),
    "提供 WebXR 头显入口",
  );
  ensure(
    !(await page.getByLabel("给 Norma 的指令", { exact: true }).isVisible()),
    "VR 主视野不叠加二维工作台控件",
  );
  return results;
}
