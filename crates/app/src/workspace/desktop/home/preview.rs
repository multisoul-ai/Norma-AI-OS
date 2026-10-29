//! 首页只呈现真实节点的几何示意，不嵌入项目视图或读取终端内容。
use crate::app::MultisoulApp;
use canvas::{CanvasRect, NodeId, NodeType, Vec2};
use gpui::{div, prelude::*, px, rgba, Context, Div};

struct PreviewNode {
    id: NodeId,
    bounds: CanvasRect,
    kind: NodeType,
    z: i32,
}

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn desktop_home_preview(
        &self,
        index: usize,
        workspace: &storage::DesktopWorkspace,
        content: CanvasRect,
        full: Vec2,
        cx: &mut Context<Self>,
    ) -> Div {
        let areas = self.desktop_workspace.previews.get(&workspace.id);
        let areas = areas.map(Vec::as_slice).unwrap_or_default();
        let rects = self.desktop_project_rects(
            &workspace.id,
            full,
            areas.iter().map(|area| area.id.as_str()),
        );
        let rects = super::super::layout::home_project_preview_layout(content.size, &rects);
        div()
            .debug_selector(move || format!("desktop-workspace-content-{index}"))
            .absolute()
            .left(px(content.origin.x))
            .top(px(content.origin.y))
            .w(px(content.size.x))
            .h(px(content.size.y))
            .children(
                areas
                    .iter()
                    .zip(rects)
                    .enumerate()
                    .map(|(area_index, (area, rect))| {
                        let mut nodes = self.home_preview_nodes(&workspace.id, area, cx);
                        nodes.sort_by_key(|node| (node.z, node.id));
                        let bounds: Vec<_> = nodes.iter().map(|node| node.bounds).collect();
                        let bounds =
                            super::super::layout::home_project_preview_layout(rect.size, &bounds);
                        div()
                            .debug_selector(move || {
                                format!("desktop-workspace-preview-{index}-{area_index}")
                            })
                            .absolute()
                            .left(px(rect.origin.x))
                            .top(px(rect.origin.y))
                            .w(px(rect.size.x))
                            .h(px(rect.size.y))
                            .when(nodes.is_empty(), |view| {
                                view.rounded(px(6.0)).bg(rgba(0x10161eff))
                            })
                            .children(nodes.into_iter().zip(bounds).map(|(node, bounds)| {
                                schematic_window(index, area_index, node, bounds)
                            }))
                    }),
            )
    }

    fn home_preview_nodes(
        &self,
        workspace: &str,
        area: &storage::DesktopAreaPreview,
        cx: &gpui::App,
    ) -> Vec<PreviewNode> {
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace.to_owned(), area.id.clone()))
        {
            // 已加载但清空的会话也是权威数据，不能回退到旧保存快照。
            return session
                .read(cx)
                .runtime
                .document
                .nodes()
                .iter()
                .map(|node| PreviewNode {
                    id: node.id,
                    bounds: node.bounds,
                    kind: node.kind.node_type(),
                    z: node.z_index,
                })
                .collect();
        }
        area.nodes
            .iter()
            .map(|node| PreviewNode {
                id: node.id,
                bounds: node.bounds,
                kind: node.kind.node_type(),
                z: node.z_index,
            })
            .collect()
    }
}

fn schematic_window(index: usize, area: usize, node: PreviewNode, bounds: CanvasRect) -> Div {
    let id = node.id;
    // 极小窗口按同比例减小细节；不改变窗口本身的真实几何或省略节点。
    let detail = (bounds.size.x / 64.0)
        .min(bounds.size.y / 42.0)
        .min(1.0)
        .max(0.0);
    let tint = match node.kind {
        NodeType::Term => 0xa8c7faff,
        NodeType::Agent => 0xc4b5fdff,
        NodeType::Browser => 0x8ddbb7ff,
        NodeType::Preview => 0xf2ce91ff,
        NodeType::Rect => 0xc4c7cfff,
    };
    let width = (bounds.size.x - 14.0 * detail).max(0.0);
    div()
        .debug_selector(move || format!("desktop-home-node-{index}-{area}-{id}"))
        .absolute()
        .left(px(bounds.origin.x))
        .top(px(bounds.origin.y))
        .w(px(bounds.size.x))
        .h(px(bounds.size.y))
        .rounded(px(6.0 * detail))
        .bg(rgba(0x141a22ff))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(10.0 * detail))
                // GPUI overflow 只有矩形裁剪，标题子层自己绘制两个外侧圆角。
                .rounded_tl(px(6.0 * detail))
                .rounded_tr(px(6.0 * detail))
                .bg(rgba(0x1a212aff))
                .child(
                    div()
                        .absolute()
                        .left(px(6.0 * detail))
                        .top(px(3.0 * detail))
                        .size(px(4.0 * detail))
                        .rounded_full()
                        .bg(rgba(tint)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(14.0 * detail))
                        .top(px(4.0 * detail))
                        .w(px(width * 0.4))
                        .h(px(2.0 * detail))
                        .rounded_full()
                        .bg(rgba(0xe3e3e840)),
                ),
        )
        .children(
            [0.72, 0.50, 0.64, 0.36]
                .into_iter()
                .enumerate()
                .map(|(i, ratio)| {
                    div()
                        .absolute()
                        .left(px(7.0 * detail))
                        .top(px((16.0 + 6.0 * i as f32) * detail))
                        .w(px(width * ratio))
                        .h(px(2.0 * detail))
                        .rounded_full()
                        .bg(rgba(match i {
                            0 => 0xe3e3e838,
                            1 => (tint & 0xffffff00) | 0x55,
                            _ => 0xe3e3e824,
                        }))
                }),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(6.0 * detail))
                .border(px(detail))
                .border_color(rgba(0xffffff0f)),
        )
}
