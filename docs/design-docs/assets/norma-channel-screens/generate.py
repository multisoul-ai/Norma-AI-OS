"""Generate the seven static iPhone screens for the Norma channel concept."""

from base64 import b64encode
from html import escape
from pathlib import Path


OUT = Path(__file__).parent
INK = "#f5f5f5"
MUTED = "#929497"
SOFT = "#c6c7c9"
GREEN = "#c9e8d6"
BLUE = "#bdd3f5"
AMBER = "#efd2a6"
RED = "#f0b8b1"
AVATAR = b64encode(
    (OUT.parent / "norma-realtime" / "cua-companion-larger-v54.png").read_bytes()
).decode("ascii")


class Screen:
    def __init__(self, title):
        self.title = title
        self.parts = [
            '<svg xmlns="http://www.w3.org/2000/svg" width="390" height="844" '
            'viewBox="0 0 390 844" role="img" aria-label="'
            + escape(title, quote=True)
            + '">',
            '<title>' + escape(title) + '</title>',
            '<rect width="390" height="844" fill="#08090a"/>',
            '<style>text{font-family:-apple-system,BlinkMacSystemFont,"SF Pro Display",'
            '"PingFang SC",sans-serif}</style>',
        ]
        self.text(25, 30, "9:41", 15, INK, 650)
        self.rect(145, 11, 100, 29, 15, "#020303")
        self.text(329, 30, "●●  ▰", 13, INK, 600)
        self.text(320, 73, "概念稿", 10, MUTED, 550)

    def rect(self, x, y, w, h, r=0, fill="#18191b", stroke=None, sw=1):
        border = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ""
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{fill}"{border}/>'
        )

    def text(self, x, y, value, size=14, fill=INK, weight=400, anchor=None):
        a = f' text-anchor="{anchor}"' if anchor else ""
        self.parts.append(
            f'<text x="{x}" y="{y}" font-size="{size}" fill="{fill}" '
            f'font-weight="{weight}"{a}>{escape(value)}</text>'
        )

    def line(self, x1, y1, x2, y2, color="#2b2d30", sw=1):
        self.parts.append(
            f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" '
            f'stroke="{color}" stroke-width="{sw}"/>'
        )

    def circle(self, x, y, r, fill):
        self.parts.append(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{fill}"/>')

    def pill(self, x, y, w, label, fill="#202225", color=SOFT, border=None):
        self.rect(x, y, w, 26, 13, fill, border)
        self.text(x + w / 2, y + 18, label, 11, color, 600, "middle")

    def header(self, title, subtitle=None, back=None):
        if back:
            self.text(24, 80, "‹  " + back, 14, GREEN, 550)
            y = 119
        else:
            y = 107
        self.text(24, y, title, 27, INK, 680)
        if subtitle:
            self.text(24, y + 28, subtitle, 12, MUTED)

    def nav(self, active):
        self.rect(0, 769, 390, 75, 0, "#111214")
        self.line(0, 769, 390, 769, "#2a2c2e")
        self.text(83, 798, "▦", 21, GREEN if active == "channels" else MUTED, 500, "middle")
        self.text(83, 820, "频道", 10, GREEN if active == "channels" else MUTED, 600, "middle")
        self.circle(195, 792, 22, "#d9e9de")
        self.text(195, 799, "N", 19, "#172019", 700, "middle")
        self.text(195, 825, "Norma", 10, SOFT, 600, "middle")
        self.text(307, 798, "▣", 21, GREEN if active == "inbox" else MUTED, 500, "middle")
        self.text(307, 820, "收件箱", 10, GREEN if active == "inbox" else MUTED, 600, "middle")
        self.rect(146, 833, 98, 5, 3, "#d8d8d8")

    def home(self):
        self.rect(146, 830, 98, 5, 3, "#d8d8d8")

    def write(self, name):
        self.parts.append("</svg>")
        (OUT / name).write_text("\n".join(self.parts) + "\n", encoding="utf-8")


def channels():
    s = Screen("Norma OS · 频道列表")
    s.header("频道", "每件事都有自己的工作现场")
    s.rect(24, 155, 342, 44, 16, "#1b1c1f", "#37393c")
    s.text(42, 183, "⌕  搜索频道、消息或成果", 13, MUTED)
    s.pill(24, 216, 57, "全部", "#dfece3", "#172019")
    s.pill(89, 216, 71, "进行中")
    s.pill(168, 216, 80, "待我处理")
    s.pill(256, 216, 72, "已归档")
    s.rect(24, 266, 342, 164, 21, "#19221d", "#4a6953")
    s.text(44, 298, "改进 iOS 首屏交接", 18, INK, 650)
    s.pill(275, 281, 70, "执行中", "#2c4234", GREEN)
    s.text(44, 326, "iOS App  ·  API  ·  2 位 Agent", 12, GREEN)
    s.line(44, 343, 345, 343, "#385043")
    s.circle(54, 368, 11, GREEN)
    s.text(54, 372, "N", 10, "#172019", 700, "middle")
    s.text(75, 371, "Norma 已收到两条真实派工回执。", 12, SOFT)
    s.text(44, 401, "查看频道对话与当前任务", 12, MUTED)
    s.text(335, 405, "›", 22, GREEN)
    s.text(24, 470, "最近的频道", 15, INK, 620)
    s.rect(24, 488, 342, 103, 18, "#17181a", "#303236")
    s.text(43, 520, "新用户首次连接体验", 16, INK, 590)
    s.pill(289, 502, 59, "讨论中")
    s.text(43, 550, "尚未开工 · 上次讨论：扫码后的引导", 12, MUTED)
    s.text(335, 559, "›", 21, MUTED)
    s.rect(24, 603, 342, 103, 18, "#17181a", "#303236")
    s.text(43, 635, "部署前检查", 16, INK, 590)
    s.pill(277, 617, 71, "待验收", "#3a3024", AMBER)
    s.text(43, 665, "1 项成果等待你查看 · 关联 Desktop", 12, MUTED)
    s.text(335, 674, "›", 21, MUTED)
    s.circle(339, 102, 19, "#e5e5e5")
    s.text(339, 109, "+", 23, "#111", 450, "middle")
    s.nav("channels")
    s.write("01-channels.svg")


def conversation():
    s = Screen("Norma OS · 频道对话")
    s.header("改进 iOS 首屏交接", "执行中 · iOS App + API", "频道")
    s.circle(35, 179, 10, GREEN)
    s.text(35, 183, "N", 9, "#172019", 700, "middle")
    s.circle(60, 179, 10, "#9db8e8")
    s.text(60, 183, "C", 9, "#172019", 700, "middle")
    s.circle(85, 179, 10, "#d5baa8")
    s.text(85, 183, "D", 9, "#172019", 700, "middle")
    s.text(105, 183, "Norma、Codex、Design", 11, MUTED)
    s.pill(263, 165, 103, "与 Norma 通话", "#dfece3", "#172019")
    s.rect(24, 207, 342, 104, 18, "#19231d", "#4a6953")
    s.text(43, 235, "共享上下文", 12, GREEN, 650)
    s.text(43, 258, "目标：让手机接续桌面工作更清楚", 13, INK)
    s.text(43, 282, "决定 2 条  ·  硬约束 1 条", 11, SOFT)
    s.text(341, 283, "›", 19, GREEN)
    s.text(24, 344, "今天 09:41 · 实时通话转写", 11, MUTED)
    s.circle(38, 374, 13, "#646b77")
    s.text(38, 378, "我", 10, INK, 600, "middle")
    s.text(61, 370, "你", 12, SOFT, 600)
    s.text(61, 392, "@Norma，把首屏交接做顺，", 13, INK)
    s.text(61, 412, "iOS 和 API 都看一下。", 13, INK)
    s.circle(38, 457, 13, GREEN)
    s.text(38, 461, "N", 10, "#172019", 700, "middle")
    s.text(61, 452, "Norma", 12, GREEN, 650)
    s.text(61, 475, "我会让 Design 检查引导，Codex 处理", 13, INK)
    s.text(61, 495, "实现；两个项目已关联到频道。", 13, INK)
    s.text(61, 517, "2 条派工接收回执  ·  查看依据", 11, MUTED)
    s.rect(24, 540, 342, 93, 17, "#1b1e25", "#405273")
    s.rect(24, 540, 4, 93, 2, "#aac5ee")
    s.text(43, 566, "Codex · 执行中", 11, BLUE, 650)
    s.text(43, 590, "调整 iOS 入口和状态提示", 14, INK, 600)
    s.text(43, 612, "方案、进展和追问在原任务里  ›", 11, MUTED)
    s.circle(38, 662, 13, "#d5baa8")
    s.text(38, 666, "D", 10, "#172019", 700, "middle")
    s.text(61, 657, "Design", 12, AMBER, 650)
    s.text(61, 679, "已整理 3 处引导文案，等你看预览。", 12, INK)
    s.rect(24, 721, 342, 50, 22, "#1d1e21", "#434549")
    s.text(42, 752, "发消息，或 @ Agent…", 13, MUTED)
    s.circle(339, 746, 17, "#e6e6e6")
    s.text(339, 752, "↑", 19, "#111", 600, "middle")
    s.home()
    s.write("02-conversation.svg")


def context():
    s = Screen("Norma OS · 频道共享上下文")
    s.header("频道上下文", "所有参与 Agent 共享这些已确认事实", "改进 iOS 首屏交接")
    s.rect(24, 172, 342, 111, 18, "#19231d", "#527059")
    s.text(43, 202, "目标", 12, GREEN, 650)
    s.text(43, 229, "让新用户从 Mac 切到 iPhone 时，", 14, INK)
    s.text(43, 252, "能看懂当前任务并继续操作。", 14, INK)
    s.text(43, 272, "来源：你 · 今天 09:41", 10, MUTED)
    s.rect(24, 296, 342, 105, 18, "#26211c", "#6b5641")
    s.text(43, 326, "硬约束 · 仅用户可修改", 12, AMBER, 650)
    s.text(43, 354, "禁止直接发布生产环境。", 14, INK)
    s.text(43, 381, "来源：你 · 昨天 21:10", 10, MUTED)
    s.text(24, 441, "已确认决定", 16, INK, 650)
    s.rect(24, 458, 342, 107, 18, "#181a1c", "#323538")
    s.circle(44, 484, 4, GREEN)
    s.text(57, 488, "首屏展示任务状态与最近成果", 13, INK)
    s.text(57, 512, "Norma 更新 · 基于频道讨论 #18", 10, MUTED)
    s.line(43, 526, 346, 526)
    s.circle(44, 545, 4, GREEN)
    s.text(57, 549, "交接卡只保留一个主操作", 13, INK)
    s.text(24, 604, "关联资源", 16, INK, 650)
    s.pill(24, 623, 100, "iOS App", "#233028", GREEN)
    s.pill(133, 623, 76, "API", "#233028", GREEN)
    s.pill(218, 623, 115, "3 位参与者", "#252629", SOFT)
    s.rect(24, 673, 342, 88, 17, "#151719", "#2d3032")
    s.text(43, 701, "上下文如何给 Agent", 12, SOFT, 620)
    s.text(43, 725, "目标、决定、来源摘要 + 最近相关消息；", 11, MUTED)
    s.text(43, 744, "更早记录与项目文件按需读取。", 11, MUTED)
    s.home()
    s.write("03-shared-context.svg")


def orchestration():
    s = Screen("Norma OS · 自主编排")
    s.header("Norma 派工", "频道：改进 iOS 首屏交接", "频道")
    s.rect(24, 164, 342, 111, 19, "#1d2821", "#53735c")
    s.text(44, 196, "已开始执行", 20, INK, 660)
    s.text(44, 222, "根据频道目标，分成设计与实现两项。", 12, SOFT)
    s.pill(44, 239, 75, "2 个任务", "#304535", GREEN)
    s.pill(128, 239, 93, "2 条接收回执", "#304535", GREEN)
    s.text(24, 316, "Norma 的安排", 16, INK, 650)
    s.line(42, 346, 42, 610, "#3c5543", 2)
    s.circle(42, 354, 7, GREEN)
    s.text(62, 351, "09:41 · 确认执行意图", 13, INK, 600)
    s.text(62, 374, "保存想法之后，你明确说“做这个”。", 11, MUTED)
    s.circle(42, 414, 7, GREEN)
    s.text(62, 411, "09:42 · 关联两个项目", 13, INK, 600)
    s.text(62, 434, "iOS App 与 API · 变更原因已记录", 11, MUTED)
    s.circle(42, 475, 7, BLUE)
    s.text(62, 472, "09:42 · 派给 Design", 13, INK, 600)
    s.text(62, 495, "检查交接动线与文案 · 已接收", 11, MUTED)
    s.circle(42, 536, 7, BLUE)
    s.text(62, 533, "09:43 · 派给 Codex", 13, INK, 600)
    s.text(62, 556, "调整入口与状态接口 · 已接收", 11, MUTED)
    s.rect(24, 632, 342, 126, 18, "#191b1d", "#363a3b")
    s.text(43, 661, "为什么这样分工？", 13, GREEN, 650)
    s.text(43, 688, "Design 先保证交接步骤可理解；Codex", 12, INK)
    s.text(43, 708, "并行处理 iOS 与 API 的可见状态。", 12, INK)
    s.text(43, 736, "普通范围自主调整，硬约束保持有效。", 11, MUTED)
    s.home()
    s.write("04-norma-orchestration.svg")


def task():
    s = Screen("Norma OS · Agent 任务详情")
    s.header("调整 iOS 入口和状态", "Codex · 来自频道「改进 iOS 首屏交接」", "频道")
    s.pill(24, 165, 69, "执行中", "#2a3c31", GREEN)
    s.pill(101, 165, 77, "任务 #02")
    s.rect(24, 210, 342, 138, 19, "#1a1e25", "#435371")
    s.text(43, 240, "当前阶段", 11, BLUE, 660)
    s.text(43, 272, "实现与验证", 20, INK, 650)
    s.text(43, 301, "完成入口状态调整；正在核对 API 返回。", 12, SOFT)
    s.rect(43, 321, 296, 5, 3, "#39404a")
    s.rect(43, 321, 190, 5, 3, BLUE)
    s.text(24, 391, "任务动态", 16, INK, 650)
    s.line(24, 408, 366, 408)
    s.circle(40, 439, 9, "#2d4938")
    s.text(40, 443, "✓", 12, GREEN, 700, "middle")
    s.text(61, 437, "09:43 · 已接收任务", 13, INK, 590)
    s.text(61, 458, "真实 Agent 回执 · 查看来源", 10, MUTED)
    s.line(24, 478, 366, 478)
    s.circle(40, 509, 9, "#2d4938")
    s.text(40, 513, "✓", 12, GREEN, 700, "middle")
    s.text(61, 507, "10:05 · 修改 3 处入口状态", 13, INK, 590)
    s.text(61, 528, "可查看 diff 与相关文件", 10, MUTED)
    s.line(24, 548, 366, 548)
    s.circle(40, 579, 9, "#343947")
    s.text(40, 583, "…", 12, BLUE, 700, "middle")
    s.text(61, 577, "10:21 · 正在验证两端数据", 13, INK, 590)
    s.text(61, 598, "尚无完成回执", 10, MUTED)
    s.rect(24, 642, 342, 59, 17, "#15191a", "#354238")
    s.text(43, 667, "你可以继续追问或补充", 12, GREEN, 620)
    s.text(43, 687, "消息仍回到这张任务卡，不新建任务。", 11, MUTED)
    s.rect(24, 720, 342, 49, 22, "#1d1e21", "#434549")
    s.text(42, 750, "给 Codex 发消息…", 13, MUTED)
    s.circle(339, 744, 16, "#e6e6e6")
    s.text(339, 750, "↑", 18, "#111", 600, "middle")
    s.home()
    s.write("05-agent-task.svg")


def inbox():
    s = Screen("Norma OS · 收件箱")
    s.header("收件箱", "只显示需要你处理的事项")
    s.pill(24, 156, 71, "待处理 3", "#e8e8e8", "#151515")
    s.pill(103, 156, 72, "已处理")
    s.rect(24, 199, 342, 158, 20, "#251f1a", "#6f5946")
    s.text(43, 229, "待验收  ·  改进 iOS 首屏交接", 11, AMBER, 650)
    s.text(43, 263, "首屏交接新版本已准备好", 16, INK, 620)
    s.text(43, 288, "有手机预览、改动说明与验证证据。", 12, SOFT)
    s.text(43, 330, "查看成果并验收", 12, AMBER, 650)
    s.text(338, 331, "›", 20, AMBER)
    s.rect(24, 370, 342, 153, 20, "#1b1e24", "#465370")
    s.text(43, 400, "需要决定  ·  部署前检查", 11, BLUE, 650)
    s.text(43, 433, "先向内测用户开放吗？", 16, INK, 620)
    s.text(43, 458, "Norma 已整理影响范围和两个选择。", 12, SOFT)
    s.text(43, 497, "查看判断依据", 12, BLUE, 650)
    s.text(338, 498, "›", 20, BLUE)
    s.rect(24, 536, 342, 153, 20, "#251d1d", "#734e4d")
    s.text(43, 566, "执行失败  ·  项目依赖更新", 11, RED, 650)
    s.text(43, 599, "测试环境无法下载依赖", 16, INK, 620)
    s.text(43, 624, "任务已停下，未将失败标为完成。", 12, SOFT)
    s.text(43, 663, "查看原因与下一步", 12, RED, 650)
    s.text(338, 664, "›", 20, RED)
    s.nav("inbox")
    s.write("06-inbox.svg")


def review():
    s = Screen("Norma OS · 成果验收")
    s.header("验收成果", "改进 iOS 首屏交接 · 第 2 版", "收件箱")
    s.rect(24, 164, 342, 296, 20, "#17191b", "#383b3d")
    s.text(43, 193, "手机预览", 12, SOFT, 630)
    s.text(335, 193, "↗", 16, GREEN)
    s.rect(97, 211, 196, 229, 26, "#08090a", "#787c7e", 2)
    s.rect(162, 218, 65, 14, 7, "#020303")
    s.text(115, 253, "从电脑继续", 16, INK, 650)
    s.text(115, 272, "任务状态和下一步都在这里", 8, MUTED)
    s.rect(112, 285, 166, 74, 12, "#1c2820", "#4b7055")
    s.text(123, 305, "改进 iOS 首屏交接", 9, INK, 600)
    s.text(123, 326, "2 个 Agent 已完成", 8, GREEN)
    s.text(123, 343, "查看成果  ›", 8, SOFT)
    s.rect(112, 369, 166, 40, 11, "#1a1b1d", "#393b3e")
    s.text(123, 393, "最近使用的项目  ›", 8, SOFT)
    s.rect(24, 478, 342, 155, 19, "#191b1c", "#373a3a")
    s.text(43, 508, "交付与证据", 14, INK, 630)
    s.text(43, 536, "✓  iOS 入口和 API 状态已联动", 12, SOFT)
    s.text(43, 562, "✓  自动化检查通过 · 查看记录", 12, SOFT)
    s.text(43, 588, "✓  预览与改动文件可打开", 12, SOFT)
    s.text(43, 614, "执行完成 ≠ 你已验收", 11, AMBER)
    s.rect(24, 677, 148, 53, 16, "#1b1c1e", "#575a5b")
    s.text(98, 710, "退回修改", 14, INK, 620, "middle")
    s.rect(183, 677, 183, 53, 16, "#dfece3")
    s.text(274, 710, "验收通过", 14, "#172019", 700, "middle")
    s.text(24, 757, "通过后可归档频道，之后仍可重开。", 11, MUTED)
    s.home()
    s.write("07-artifact-review.svg")


def voice_base(title, state):
    s = Screen(title)
    s.text(24, 76, "‹  返回频道", 13, GREEN, 550)
    s.text(195, 76, "与 Norma 实时对话", 14, INK, 630, "middle")
    s.rect(24, 101, 342, 66, 18, "#1c261f", "#4b6755")
    s.text(43, 126, "当前频道", 10, GREEN, 640)
    s.text(43, 150, "改进 iOS 首屏交接  ·  iOS App + API", 12, INK, 590)
    s.text(24, 190, "●", 11, GREEN)
    s.text(42, 190, state, 12, SOFT)
    s.parts.append(
        '<defs><clipPath id="portrait-clip"><rect x="26" y="201" width="338" '
        'height="342" rx="20"/></clipPath></defs>'
    )
    s.parts.append(
        '<image x="-45" y="-12" width="480" height="1027" '
        'href="data:image/jpeg;base64,' + AVATAR + '" clip-path="url(#portrait-clip)"/>'
    )
    s.text(195, 554, "N O R M A", 11, SOFT, 500, "middle")
    return s


def voice_listening():
    s = voice_base("Norma OS · 频道内实时语音", "正在聆听 · 你可以随时插话")
    s.rect(24, 577, 342, 99, 18, "#1b1d20", "#3a3d40")
    s.text(43, 603, "你 · 实时转写", 11, BLUE, 650)
    s.text(43, 627, "“让 Codex 再检查一下 API 状态，", 13, INK)
    s.text(43, 650, "   直接接着刚才的任务做。”", 13, INK)
    s.text(24, 704, "Norma 会把指令送回原任务并记录回执", 11, MUTED)
    s.circle(84, 751, 26, "#212326")
    s.text(84, 757, "⌁", 20, INK, 500, "middle")
    s.text(84, 794, "静音", 11, MUTED, 550, "middle")
    s.circle(195, 751, 26, "#212326")
    s.text(195, 757, "≡", 20, INK, 500, "middle")
    s.text(195, 794, "字幕", 11, MUTED, 550, "middle")
    s.circle(306, 751, 26, "#a74743")
    s.text(306, 757, "×", 23, INK, 450, "middle")
    s.text(306, 794, "挂断", 11, MUTED, 550, "middle")
    s.home()
    s.write("08-realtime-voice.svg")


def voice_report():
    s = voice_base("Norma OS · 通话中成果汇报", "Norma 正在汇报 · 你的讲话可打断她")
    s.rect(24, 568, 342, 129, 19, "#29221c", "#775c43")
    s.text(43, 595, "需要你验收", 11, AMBER, 650)
    s.text(43, 622, "首屏交接第 2 版已完成。", 15, INK, 620)
    s.text(43, 647, "预览和证据已放进原频道。", 12, SOFT)
    s.text(43, 675, "Norma：现在查看，还是通话后再看？", 11, AMBER)
    s.rect(24, 711, 164, 47, 15, "#1d1e20", "#505357")
    s.text(106, 741, "稍后放收件箱", 12, INK, 600, "middle")
    s.rect(201, 711, 165, 47, 15, "#e6eee8")
    s.text(283, 741, "查看原任务成果", 12, "#172019", 650, "middle")
    s.text(195, 790, "通话继续 · 普通进展不会打断你", 11, MUTED, 400, "middle")
    s.home()
    s.write("09-voice-report.svg")


for render in (
    channels, conversation, context, orchestration, task, inbox, review,
    voice_listening, voice_report,
):
    render()
