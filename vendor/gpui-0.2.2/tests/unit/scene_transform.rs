use super::SceneTransform;
use crate::{
    Bounds, ContentMask, MouseButton, MouseDownEvent, MouseEvent, point, px,
    scene::{Quad, Scene},
    scene_transform_paint::ScenePlacement,
    size,
};

/// 缓存图元在父镜头变化后重新投影，裁剪与鼠标逆变换使用相同坐标。
///
/// 数据构造：局部卡片 200×100，设备比例 2，因此原始图元 400×200；
///   项目 offset=(100,60)、scale=1/2；父空间初始 offset=(400,200)、scale=1/4，
///   初始物理 bounds=(850,430,50,25)。父镜头改为 offset=(300,100)、scale=1，
///   最终物理 bounds=(800,320,200,100)，外层逻辑右界 440 → 物理右界 880。
/// 执行过程：提交原始局部 quad → 改父镜头 → 重放同一批命令 → 逆变换新位置事件。
/// 预期结果：背景图元保留 1 个，实际可见大小 80×100；不保留旧位置；
///   逻辑屏幕 (420,170) 返回卡片局部 (40,20)，不能仍返回屏幕坐标。
#[test]
fn retained_quad_tracks_parent_camera_and_clip() {
    let parent = SceneTransform::new();
    let viewport = Bounds::new(point(px(0.), px(0.)), size(px(440.), px(240.)));
    parent.update(None, 0.25, point(px(400.), px(200.)), viewport);
    let project = SceneTransform::new();
    project.update(
        Some(parent.clone()),
        0.5,
        point(px(100.), px(60.)),
        Bounds::new(point(px(0.), px(0.)), size(px(400.), px(240.))),
    );
    let logical = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(100.)));
    let mut original = Scene::default();
    original.transform = Some(ScenePlacement {
        transform: project.clone(),
        scale_factor: 2.,
    });
    original.insert_primitive(Quad {
        bounds: logical.scale(2.),
        content_mask: ContentMask {
            bounds: logical.scale(2.),
        },
        ..Default::default()
    });
    assert_eq!(original.quads.len(), 1, "初始卡片图元应被提交");
    assert_eq!(
        original.quads[0].bounds,
        Bounds::new(point(px(425.), px(215.)), size(px(25.), px(12.5))).scale(2.),
        "初始缩放应包含父子两层及设备比例"
    );
    parent.update(None, 1., point(px(300.), px(100.)), viewport);
    let mut presented = Scene::default();
    presented.replay(0..original.len(), &original);
    assert_eq!(
        presented.quads.len(),
        1,
        "重放应保持同一张卡片，不能重复生成"
    );
    let quad = &presented.quads[0];
    assert_eq!(
        quad.bounds,
        Bounds::new(point(px(400.), px(160.)), size(px(100.), px(50.))).scale(2.),
        "缓存图元应使用新父镜头"
    );
    assert_ne!(quad.bounds, original.quads[0].bounds, "不能保留旧呈现坐标");
    assert_eq!(
        quad.bounds.intersect(&quad.content_mask.bounds),
        Bounds::new(point(px(400.), px(160.)), size(px(40.), px(50.))).scale(2.),
        "父级裁剪必须限制物理右边界为 880"
    );
    let mapping = project.resolve();
    let event = MouseDownEvent {
        position: point(px(420.), px(170.)),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
        modifiers: Default::default(),
    };
    let local = event.to_local(mapping.scale, mapping.offset);
    assert_eq!(
        local.position,
        point(px(40.), px(20.)),
        "事件必须逆变换完整父子链"
    );
    assert_ne!(
        local.position, event.position,
        "缩放后的事件不能继续使用屏幕坐标"
    );
}

/// 镜头之外的局部绘制命令不能丢失，移入画面时无需重新构建内容。
///
/// 数据构造：卡片 200×100，viewport 440×240，scale=1；初始 x=-400，
///   卡片右界=-200<0，所以没有可见图元；移动到 (100,50) 后完整进入 viewport。
/// 执行过程：首次提交局部 quad → 确认可见队列为空 → 改变镜头 → 重放保留的命令。
/// 预期结果：首帧不绘制；命令仍保留 1 条；下一帧出现 1 张卡片且位于 (100,50)。
#[test]
fn clipped_commands_reappear_after_camera_moves() {
    let transform = SceneTransform::new();
    let viewport = Bounds::new(point(px(0.), px(0.)), size(px(440.), px(240.)));
    transform.update(None, 1., point(px(-400.), px(0.)), viewport);
    let logical = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(100.)));
    let mut original = Scene::default();
    original.transform = Some(ScenePlacement {
        transform: transform.clone(),
        scale_factor: 2.,
    });
    original.insert_primitive(Quad {
        bounds: logical.scale(2.),
        content_mask: ContentMask {
            bounds: logical.scale(2.),
        },
        ..Default::default()
    });
    assert!(
        original.quads.is_empty(),
        "视口外的内容不能进入 GPU 可见队列"
    );
    assert_eq!(original.len(), 1, "局部命令必须保留，以便镜头移入后重放");
    transform.update(None, 1., point(px(100.), px(50.)), viewport);
    let mut presented = Scene::default();
    presented.replay(0..original.len(), &original);
    assert_eq!(presented.quads.len(), 1, "进入视口后应恢复原图元");
    assert_eq!(
        presented.quads[0].bounds,
        Bounds::new(point(px(100.), px(50.)), size(px(200.), px(100.))).scale(2.),
        "重放必须使用移动后的屏幕位置"
    );
    presented.clear();
    assert!(
        presented.quads.is_empty(),
        "清理后不能保留已移除的 GPU 图元"
    );
    assert_eq!(presented.len(), 0, "清理后不能保留上一场景的绘制命令");
}

/// 镜头淡出必须作用于缓存绘制命令，不能复用上一帧烘焙的透明度。
///
/// 数据构造：原 quad alpha=0.8，父 opacity=0.5，子 opacity=0.25；
///   重放后的 alpha=0.8×0.5×0.25=0.1；恢复子 opacity=1 后 alpha=0.4。
/// 执行过程：保留同一 quad → 改父子透明度 → 重放 → 恢复子透明度再重放。
/// 预期结果：透明度分别为0.1和0.4；不得保持0.8或累乘上次已淡出的结果。
#[test]
fn retained_opacity_tracks_the_current_scene_without_compounding() {
    let viewport = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(100.)));
    let parent = SceneTransform::new();
    parent.update(None, 1., point(px(0.), px(0.)), viewport);
    let child = SceneTransform::new();
    child.update(Some(parent.clone()), 1., point(px(0.), px(0.)), viewport);
    let mut original = Scene::default();
    original.transform = Some(ScenePlacement { transform: child.clone(), scale_factor: 1. });
    original.insert_primitive(Quad { bounds: viewport.scale(1.), content_mask: ContentMask { bounds: viewport.scale(1.) }, border_color: crate::hsla(0., 0., 1., 0.8), ..Default::default() });
    parent.set_opacity(0.5);
    child.set_opacity(0.25);
    let mut faded = Scene::default();
    faded.replay(0..original.len(), &original);
    assert_eq!(faded.quads[0].border_color.a, 0.1, "缓存图元应使用当前父子透明度");
    assert_ne!(faded.quads[0].border_color.a, 0.8, "不得沿用未淡出的缓存颜色");
    child.set_opacity(1.);
    let mut restored = Scene::default();
    restored.replay(0..faded.len(), &faded);
    assert_eq!(restored.quads[0].border_color.a, 0.4, "重放必须从原始颜色计算，不可累乘上次结果");
}
