# 灵动岛 2.0 还原

对照 `docs/prototypes/island-2/reference/`。几何以原生窗口宽高为准，容差 ±1pt。头像轮廓由名字散列，不要求与原型的 egg/circle/hex/tri 一一对应；核对应是大小一致、配色和槽位。不画原型里的假摄像头圆点。

- `01-compact-n2.png`：330×34。左青环 + 1 + 一个 20pt 头像。右橙环 + 徽章 1。
- `02-compact-n5.png`：416×34。左 3 个重叠头像，没有 +N。右 2 个头像 + 徽章 2。
- `03-compact-n8.png`：416×34。左 3 个头像 + `+1`，数字 4。右 2 个头像 + 徽章 3。
- `04-activity.png`：460 宽。已完成、Leo 完成了 push relay、3 个文件、查看成果、底边绿线。
- `05-alert.png`：540 宽。需要你、3 件待处理、待处理队列。选中等待表情，未选中平静且全不透明。
- `06-alert-switch.png`：点击队列后当前卡换人。
- `07-expanded-running.png`：660 宽，头 52。筛选全部/运行中/等你/完成，右侧 20pt 一览。Daisy 待开始且无定位。
  每行三层：会话名 · provider；人形图标 + 最近输入；等宽最近工具（运行中圈 / 其余灰点，运行中带耗时）。Daisy 第二行尚未开始、无第三行。
- `08-filter-waiting.png`：等你筛选后只剩等你的行。
- `09-filter-hover.png` / `10-filter-pressed.png`：全部按钮的悬停与按下能和选中态分开。
- `11-expanded-pending.png`：三张待处理卡。
- `12-expanded-staff.png`：幕僚页沿用原面板，没有声波。
- `13-compact-en-light.png`：浅色主题下岛仍是黑胶囊，416×34。
- `14-filters-en.png`：All / Running / Waiting / Done，Not started，Needs you。
- `15-reduce-motion-a.png` 与 `16-reduce-motion-b.png`：400ms 内像素相同。
- `17-reduce-motion.png`：倒计时绿线铺满宽度，头像静止。

未纳入本场景的硬件：刘海屏、外接屏、全屏 Space、透明区点击穿透。幕僚汇报中的声波没有产品触发，不在这些图里。
