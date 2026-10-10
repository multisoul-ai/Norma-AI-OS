"""Render the two discussion diagrams as deterministic SVG files."""

from html import escape
from pathlib import Path

OUT = Path(__file__).resolve().parent
FONT = "-apple-system, 'PingFang SC', 'Heiti SC', Arial, sans-serif"
INK = "#182b3c"
MUTED = "#617386"
LINE = "#667e92"
BLUE = "#376dac"
TEAL = "#16756b"
RED = "#ae4f49"


class Chart:
    def __init__(self, height, title, subtitle, number, accent):
        self.height = height
        self.accent = accent
        self.nodes = []
        self.edges = []
        self.labels = []
        self.text(80, 75, f"NORMA OS  /  {number}", 20, accent, "start", 650)
        self.text(80, 139, title, 47, INK, "start", 700)
        self.text(82, 181, subtitle, 23, MUTED, "start")

    def text(self, x, y, lines, font=28, color=INK, align="middle", weight=450, gap=None):
        if isinstance(lines, str):
            lines = [lines]
        gap = gap or font * 1.48
        value = "".join(
            f'<tspan x="{x}" y="{y+i*gap}">{escape(line)}</tspan>'
            for i, line in enumerate(lines)
        )
        self.labels.append(
            f'<text font-family="{FONT}" font-size="{font}" fill="{color}" '
            f'text-anchor="{align}" font-weight="{weight}">{value}</text>'
        )

    def panel(self, x, y, w, h, title, lines, color=BLUE, fill="#f0f5fa"):
        self.nodes.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18" '
            f'fill="{fill}" stroke="{color}" stroke-width="1.6"/>'
        )
        self.text(x + 24, y + 40, title, 27, color, "start", 650)
        self.text(x + 24, y + 87, lines, 23, MUTED, "start", gap=38)

    def box(self, x, y, w, h, title, detail=None, color=None, fill="#ffffff", size=29):
        color = color or self.accent
        self.nodes.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="15" '
            f'fill="{fill}" stroke="{color}" stroke-width="2.2"/>'
        )
        details = [] if detail is None else ([detail] if isinstance(detail, str) else detail)
        total = size + len(details) * 33
        start = y + (h - total) / 2 + size * 0.86
        self.text(x + w / 2, start, title, size, color, weight=650)
        if details:
            self.text(x + w / 2, start + 38, details, 22, MUTED, gap=33)

    def diamond(self, cx, cy, w, h, title, color=None, size=27):
        color = color or self.accent
        points = f"{cx},{cy-h/2} {cx+w/2},{cy} {cx},{cy+h/2} {cx-w/2},{cy}"
        self.nodes.append(
            f'<polygon points="{points}" fill="#f3f7fa" stroke="{color}" stroke-width="2.2"/>'
        )
        self.text(cx, cy + size * 0.36, title, size, color, weight=650)

    def connector(self, x, y, label, color=BLUE):
        self.nodes.append(
            f'<circle cx="{x}" cy="{y}" r="18" fill="#f0f5fa" '
            f'stroke="{color}" stroke-width="2.2"/>'
        )
        self.text(x, y + 7, label, 22, color, weight=650)

    def arrow(self, points, color=LINE, dashed=False, label=None, at=None):
        path = "M " + " L ".join(f"{x},{y}" for x, y in points)
        dash = ' stroke-dasharray="9 7"' if dashed else ""
        marker = "red" if color == RED else "teal" if color == TEAL else "blue" if color == BLUE else "gray"
        self.edges.append(
            f'<path d="{path}" fill="none" stroke="{color}" stroke-width="2.6"{dash} '
            f'marker-end="url(#{marker})" stroke-linejoin="round"/>'
        )
        if label and at:
            self.text(*at, label, 20, color, weight=600)

    def save(self, name):
        markers = "".join(
            f'<marker id="{name}" viewBox="0 0 10 10" refX="9" refY="5" '
            f'markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M 0 0 L 10 5 L 0 10 z" fill="{color}"/></marker>'
            for name, color in (("gray", LINE), ("blue", BLUE), ("teal", TEAL), ("red", RED))
        )
        body = "".join(self.edges + self.nodes + self.labels)
        svg = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="1700" height="{self.height}" '
            f'viewBox="0 0 1700 {self.height}" role="img">'
            f'<defs>{markers}</defs><rect width="1700" height="{self.height}" fill="#fbfcfe"/>'
            f'{body}</svg>'
        )
        (OUT / f"{name}.svg").write_text(svg)


def current():
    c = Chart(2520, "当前任务指派逻辑", "正式桌面 Dock 路径 · 项目内名称解析 · 项目运行实例执行", "01  CURRENT", BLUE)
    x, w, cx = 280, 650, 605
    c.box(x, 240, w, 90, "用户在 Dock 下达指令", "例如：让 Alan 修复登录")
    c.box(x, 385, w, 105, "绑定当前 App 的项目上下文", "current_root + 本项目 Agent roster / canvas")
    c.box(x, 545, w, 105, "Norma 处理用户输入", "直接点名路由，或模型调用 typed tool")
    c.diamond(cx, 765, 570, 130, "是否产生新任务派工请求？")
    c.box(x, 885, w, 105, "在当前项目中解析 Agent 名称", "名称 / 语音别名 → Agent 定义 ID")
    c.diamond(cx, 1100, 570, 130, "定义存在且满足派工约束？")
    c.box(x, 1220, w, 105, "复用或激活定义对应的卡片", "按 definition_id 定位本画布中的会话")
    c.box(x, 1380, w, 105, "协调器接收任务并关联定义", "生成 task_id / task_ref，保存待提交记录")
    c.diamond(cx, 1650, 570, 150, "CLI 当前允许提交？")
    c.box(x, 1785, w, 105, "分阶段输入正文与发送 Enter", "成功后记录 submitted；不等待前一任务完成")
    c.box(x, 1960, w, 105, "目标卡片内的 Provider CLI 执行", "Claude / Codex 等管理自己的消息队列")
    c.box(x, 2140, w, 105, "可信 Hook / 回合事件返回", "按定义关联本地任务的状态与正文结果")
    c.box(x, 2300, w, 105, "更新本项目任务账本与界面", "Done / Failed / Interrupted；不以回复文本判成功")

    c.box(1120, 720, 480, 100, "普通回复或续聊处理", "本图只展开正式新任务派工支路", color=LINE)
    c.box(1120, 1040, 480, 120, "拒绝派工并返回原因", ["未知 / 禁用 / 未跟踪且忙碌等", "明确反馈；不产生成功回执"], color=RED, fill="#fff5f3")
    c.box(1120, 1372, 480, 120, "返回 accepted / queued 回执", ["表示协调器已接收或等待提交", "不表示 CLI 已执行，更不表示完成"], color=BLUE, fill="#f1f6fd")
    c.box(1120, 1585, 480, 130, "等待输入条件恢复", ["启动就绪 / 审批 / 用户输入占用", "条件变化后重新检查，不覆盖输入"], color=LINE)
    c.box(1120, 1950, 480, 120, "tracking = untracked", ["能确认提交，但没有可信完成证据", "不能靠静默或文本猜测已完成"], color=LINE)
    c.panel(1120, 240, 480, 345, "当前范围与跨项目缺口", ["名称约束：项目定义库内", "常规 Dock：尚未统一全空间派工", "手机：另有 area_id / node_id 路由", "旧桌面 dispatcher：仅测试特性", "同空间跨项目卡片尚未统一查重"], BLUE)

    for start, end in ((330, 385), (490, 545), (650, 700), (830, 885), (990, 1035), (1165, 1220), (1325, 1380), (1485, 1575), (1725, 1785), (1890, 1960), (2065, 2140), (2245, 2300)):
        c.arrow([(cx, start), (cx, end)])
    c.arrow([(890, 765), (1120, 765)], label="否", at=(1002, 745))
    c.text(633, 858, "是", 20, BLUE, "start")
    c.arrow([(890, 1100), (1120, 1100)], RED, label="否", at=(1002, 1080))
    c.text(633, 1192, "是", 20, BLUE, "start")
    c.arrow([(930, 1432), (1120, 1432)], BLUE, dashed=True, label="接收回执", at=(1024, 1410))
    c.arrow([(890, 1650), (1120, 1650)], label="否", at=(1002, 1630))
    c.arrow([(1360, 1585), (1360, 1535), (cx, 1535), (cx, 1575)], label="条件恢复后重查", at=(1015, 1515))
    c.text(633, 1755, "是", 20, BLUE, "start")
    c.arrow([(930, 2012), (1120, 2012)], label="无可信跟踪", at=(1025, 1990))
    c.text(640, 2110, "有可信事件", 20, BLUE, "start")
    c.text(82, 2480, "实线：运行步骤与分支   ·   虚线：接收回执   ·   当前名称解析以项目定义为范围", 22, MUTED, "start")
    c.save("current-dispatch")


def optimized():
    c = Chart(3900, "优化后的工作空间任务指派逻辑", "全空间派工与只读进度汇报 · 角色名称唯一，一个名称只对应一张卡片", "02  PROPOSED", TEAL)
    x, w, cx = 640, 570, 925

    c.panel(65, 235, 480, 190, "角色名称唯一是派工前提", ["角色名称 = 卡片主名称，一名一卡", "跨项目 / 画布 / 后台 / 未加载", "不能用不同别名掩盖角色重名"], TEAL, "#eef8f5")
    c.box(65, 475, 480, 95, "手动创建 / 改名角色与卡片", "新增卡片必须使用另一个角色名称", size=26)
    c.box(65, 625, 480, 95, "规范化主名称与中文语音别名", "已确认：两种称呼共享唯一呼叫键登记", size=26)
    c.diamond(305, 835, 465, 140, "主名称、别名或旧称呼已占用？", size=22)
    c.box(65, 970, 222, 145, "冲突：拒绝提交", ["保留输入与原卡片", "提示名称已被占用"], color=RED, fill="#fff5f3", size=23)
    c.box(320, 970, 225, 145, "可用：事务登记", ["新称呼生效", "旧称呼失效占用", "并发只能一项成功"], size=23)
    c.box(65, 1200, 480, 135, "工作空间唯一呼叫键登记", ["有效键寻址；失效键保留占用", "持久登记 + 内存有效目录投影"], fill="#edf8f4", size=26)
    c.panel(65, 1400, 480, 235, "点名简单，投递仍绑定身份", ["用户只需说：让 Alan 做", "项目 / 画布只是展示上下文", "解析后冻结目标，改名不改投递", "历史任务不转向后来同名卡片"], TEAL, "#eef8f5")
    c.box(65, 1705, 480, 105, "旧数据恢复：汇总全空间角色", ["包含未加载区域，保留现存合法名称", "登记完成前不按冲突名称派工"], size=26)
    c.diamond(305, 1910, 465, 130, "是否存在主名称或别名冲突？", size=23)
    c.box(65, 2040, 480, 125, "系统自动分配唯一名称", ["固定顺序保留一个原名称", "其他角色使用未占用的合法名称"], size=26)
    c.box(65, 2240, 480, 125, "事务登记并展示名称变更", ["角色名称与卡片标题同步更新", "已有任务仍绑定原卡片，不重发"], size=26)
    c.panel(65, 2430, 480, 260, "已确认：旧称呼失效占用", ["旧主名称与旧语音别名不再寻址", "原角色仍存在：其他角色不得复用", "彻底删除：释放当前与历次旧称呼", "已有任务绑定原身份；尚未改 App"], TEAL, "#eef8f5")
    c.panel(65, 2750, 480, 260, "已确认：反馈与重试", ["Dock 逐项持续更新状态", "完成 / 失败：摘要与结果入口", "明确失败：点击安全重试", "结果未知：先核对，不自动重发"], TEAL, "#eef8f5")

    c.box(x, 240, w, 105, "用户从 Dock / 手机发送消息", ["固定当前工作空间与来源操作身份", "desktop_workspace_id + operation_id"], size=28)
    c.diamond(cx, 475, 530, 140, "同一操作是否已被接收？", size=26)
    c.box(x, 625, w, 105, "工作空间级 Norma 解析意图", "派工 / 续聊 / 进度查询 / 普通讨论", size=28)
    c.diamond(cx, 860, 530, 130, "需要派工或定向回复？", size=26)
    c.box(x, 1010, w, 125, "解析称呼或项目内候选卡片", ["点名：唯一呼叫键定位同一卡片", "仅项目：唯一自动，多张选择，零张补全"], size=26)
    c.diamond(cx, 1250, 530, 140, "唯一卡片已确定且上下文一致？", size=24)
    c.box(x, 1410, w, 115, "绑定卡片身份，按需加载目标区域", ["workspace / area / card + 会话指纹", "不依赖当前展开项目，不自动换卡片"], size=26)
    c.diamond(cx, 1660, 530, 140, "目标仍有效且允许执行？", size=26)
    c.box(x, 1820, w, 115, "预检后原子接收 assignments", ["冻结目标，记录 assignment_id 与幂等键", "后台事务；已接收不等于卡片已提交"], size=27)
    c.diamond(cx, 2100, 530, 140, "目标 CLI 当前允许提交？", size=25)
    c.box(x, 2260, w, 125, "精确卡片调度，分阶段写入与 Enter", ["任务绑定 task_ref；续聊绑定 turn_id", "沿用 CLI 队列，不等待前任务结束"], size=25)
    c.box(x, 2425, w, 105, "目标项目 Runtime / Provider 执行", "submitted / running / waiting_approval", size=27)
    c.box(x, 2600, w, 130, "Dock 逐项更新进度与终态摘要", ["可信 task / turn 事件；归原工作空间", "附结果入口，点击才打开卡片"], size=26)
    c.diamond(cx, 2870, 530, 140, "任务失败且可安全重试？", size=25)
    c.box(x, 2970, w, 70, "保留结果；点击才打开目标卡片", size=25)

    rx, rw = 1275, 360
    c.box(rx, 405, rw, 145, "返回原操作回执", ["同参数：查询原结果", "参数不同：幂等冲突", "均不重复输入卡片"], color=LINE, size=26)
    c.diamond(1455, 860, 340, 125, "全空间进度查询？", color=BLUE, size=23)
    c.connector(1665, 860, "Q")
    c.box(rx, 950, rw, 105, "普通讨论", "只回复，不创建任务", color=LINE, size=26)
    c.box(rx, 1180, rw, 155, "不派工，保留任务内容", ["无有效称呼 / 零候选 / 多候选", "旧称呼失效 / 冲突 / 项目不匹配"], color=RED, fill="#fff5f3", size=26)
    c.box(rx, 1360, rw, 145, "提示补全或选择有效卡片", ["零候选补全；多候选明确选择", "选定后重验，不自动创建或换人"], color=LINE, size=24)
    c.box(rx, 1590, rw, 140, "拒绝失效目标", ["移除 / 停用 / 会话改变", "不投给同名新卡片"], color=RED, fill="#fff5f3", size=26)
    c.box(rx, 1818, rw, 120, "已接收回执", ["Norma 接管任务", "尚不代表 CLI 提交"], color=TEAL, fill="#eef8f5", size=26)
    c.box(rx, 2030, rw, 140, "等待提交", ["启动 / 审批 / 用户输入占用", "恢复后复核同一卡片"], color=LINE, size=26)
    c.box(rx, 2600, rw, 130, "跟踪证据不足", ["untracked / outcome_unknown", "先核对；不猜完成、不重发"], color=LINE, size=26)
    c.box(rx, 2800, rw, 140, "用户点击手动重试", ["保留原身份，重新复核目标", "仅失败项，成功项不重发"], color=TEAL, fill="#eef8f5", size=24)

    c.arrow([(305, 570), (305, 625)], TEAL)
    c.arrow([(305, 720), (305, 765)], TEAL)
    c.arrow([(305, 905), (305, 933), (176, 933), (176, 970)], RED, label="是", at=(204, 925))
    c.arrow([(305, 905), (305, 933), (432, 933), (432, 970)], TEAL, label="否", at=(407, 925))
    c.arrow([(432, 1115), (432, 1160), (305, 1160), (305, 1200)], TEAL)
    c.arrow([(545, 1267), (592, 1267), (592, 1062), (640, 1062)], TEAL, dashed=True)
    c.text(576, 1155, "角色登记", 19, TEAL, "middle")
    c.arrow([(305, 1810), (305, 1845)], TEAL)
    c.arrow([(305, 1975), (305, 2040)], TEAL, label="是", at=(334, 2015))
    c.arrow([(537.5, 1910), (580, 1910), (580, 2302), (545, 2302)], TEAL, label="否", at=(568, 1888))
    c.arrow([(305, 2165), (305, 2240)], TEAL)
    c.arrow([(65, 2302), (40, 2302), (40, 1267), (65, 1267)], TEAL, dashed=True)
    c.text(305, 2397, "保存后汇入同一角色登记", 20, TEAL)

    for start, end in ((345, 405), (545, 625), (730, 795), (925, 1010), (1135, 1180), (1320, 1410), (1525, 1590), (1730, 1820), (1935, 2030), (2170, 2260), (2385, 2425), (2530, 2600)):
        c.arrow([(cx, start), (cx, end)], TEAL)
    c.arrow([(1190, 475), (1275, 475)], label="是", at=(1232, 455))
    c.text(951, 585, "否：新操作", 20, TEAL, "start")
    c.arrow([(1190, 860), (1285, 860)], label="否", at=(1232, 840))
    c.arrow([(1625, 860), (1647, 860)], BLUE, label="是", at=(1643, 835))
    c.arrow([(1455, 922.5), (1455, 950)], label="否", at=(1485, 945))
    c.text(951, 965, "是", 20, TEAL, "start")
    c.arrow([(1190, 1250), (1275, 1250)], RED, label="否", at=(1232, 1230))
    c.arrow([(1455, 1335), (1455, 1360)])
    c.arrow([(1635, 1432), (1670, 1432), (1670, 1145), (1238, 1145), (1238, 1062), (1210, 1062)], TEAL, label="明确选定后重新解析", at=(1460, 1125))
    c.text(951, 1364, "是", 20, TEAL, "start")
    c.arrow([(1190, 1660), (1275, 1660)], RED, label="否", at=(1232, 1640))
    c.text(951, 1800, "是", 20, TEAL, "start")
    c.arrow([(1210, 1878), (1275, 1878)], TEAL, dashed=True)
    c.arrow([(1190, 2100), (1275, 2100)], label="否", at=(1232, 2080))
    c.arrow([(1455, 2030), (1455, 1985), (cx, 1985), (cx, 2030)], label="条件恢复后重查", at=(1230, 1968))
    c.text(951, 2215, "是", 20, TEAL, "start")
    c.arrow([(1210, 2665), (1275, 2665)], label="无充分证据", at=(1243, 2566))
    c.arrow([(cx, 2730), (cx, 2800)], TEAL, label="终态已确认", at=(1033, 2770))
    c.arrow([(1190, 2870), (1275, 2870)], TEAL, label="是", at=(1232, 2850))
    c.arrow([(cx, 2940), (cx, 2970)], TEAL)
    c.text(951, 2962, "否", 20, TEAL, "start")
    c.arrow([(1635, 2870), (1670, 2870), (1670, 1765), (615, 1765), (615, 1660), (660, 1660)], TEAL, dashed=True, label="新尝试：复核原目标", at=(1430, 1751))

    c.panel(65, 3070, 1570, 95, "两层精准性", ["唯一角色名称明确对应卡片；稳定卡片身份保证异步提交、改名、重试与结果回流都命中原目标。"], TEAL, "#eef8f5")

    c.connector(95, 3220, "Q")
    c.text(135, 3230, "只读全空间进度汇报分支", 29, BLUE, "start", 650)
    c.text(65, 3275, "固定发送时工作空间与授权范围；覆盖全部项目、后台与未加载区域，展开项目不缩小范围。", 23, MUTED, "start")
    c.box(65, 3320, 480, 190, "读取全部已知工作事实", ["当前 Runtime / Hook 与任务账本", "未加载区域：只读持久快照", "含直接卡片工作与未关联活动"], color=BLUE, size=26)
    c.box(615, 3320, 480, 190, "校验身份、合并与去重", ["任务、角色、内部步骤分列", "注明来源时间与覆盖缺口", "未知 / 过时不猜完成比例"], color=BLUE, size=26)
    c.box(1165, 3320, 470, 190, "聚合空间与项目进度", ["全部当前未结束任务与已知异常", "成果默认最近 24 小时", "只呈现有证据的等待原因"], color=BLUE, size=26)
    c.arrow([(545, 3415), (615, 3415)], BLUE)
    c.arrow([(1095, 3415), (1165, 3415)], BLUE)
    c.box(65, 3575, 1570, 120, "来源 Dock：总览 → 项目 → 角色 / 任务", ["成果摘要 + 待处理事项 + 原结果入口", "带查询与观察时间；再次询问取得新快照"], color=BLUE, size=29)
    c.arrow([(1400, 3510), (1400, 3540), (850, 3540), (850, 3575)], BLUE)
    c.panel(65, 3735, 1570, 145, "只读查询合同", ["零任务创建、零卡片输入、零 PTY 启动；不自动审批、催办或重试；点击入口才导航。", "桌面与手机共用事实，手机仅返回授权范围；默认一次查询一次快照，不增加周期推送。"], BLUE)
    c.save("optimized-dispatch")


if __name__ == "__main__":
    current()
    optimized()
