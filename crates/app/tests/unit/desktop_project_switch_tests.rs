use super::*;

/// 同级三项目在共同镜头中重定向，位置、速度、透明度与比例保持连续。
///
/// 数据构造：视口1200×700；A/B/C 锚点x分别为0/1/2，间距1200；
///   每帧16ms，5帧=80ms，小于180ms淡入和350ms尺寸过渡。
/// 执行过程：A→B推进5帧 → 中途转C → 再反向A → 推进100帧=1600ms落定。
/// 预期结果：改目标时A/B几何和透明度不跳；速度继续；A落定可见且原尺寸，B/C不可见。
#[test]
fn desktop_sibling_camera_retargets_without_dropping_visible_projects() {
    let full = CanvasRect::new(14.0, 36.0, 1200.0, 700.0);
    let layers = ["a", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(i, id)| ProjectLayer {
            id: id.into(),
            anchor: Vec2::new(i as f32, 0.0),
            opacity: if i == 0 { 1.0 } else { 0.0 },
            scale: if i == 0 { 1.0 } else { INACTIVE_SCALE },
            start: [0.0; 2],
            target: [0.0; 2],
            elapsed: 0.0,
        })
        .collect();
    let subject = |id: &str| DesktopCanvasSubject::Project {
        workspace_id: "workspace".into(),
        area_id: id.into(),
    };
    let mut projects = ProjectSwitch {
        layers,
        logical_size: full.size,
    };
    projects.select("b");
    let mut camera = DesktopCamera::new(full);
    camera.retarget(projects.camera_target("b", full), false);
    let mut transition = DesktopCanvasTransition {
        subject: subject("b"),
        camera,
        projects: Some(projects),
        directory_content: None,
    };
    for _ in 0..5 {
        transition.tick(Duration::from_millis(16));
    }
    let previous = transition.camera.rect();
    let layers_before = transition.projects.as_ref().unwrap().layers.clone();
    assert!(previous.origin.x < full.origin.x, "镜头应已向B方向移动");
    assert_ne!(previous.size, Vec2::ZERO, "同级切换不能先缩回总览");
    assert!(
        layers_before[0].opacity > 0.0 && layers_before[1].opacity > 0.0,
        "80ms时真实A/B都应在淡入淡出中"
    );
    transition.retarget(subject("c"), full);
    assert_eq!(
        transition.camera.rect(),
        previous,
        "选择C时不得重置共同镜头"
    );
    assert_eq!(
        transition.projects.as_ref().unwrap().layers[0].opacity,
        layers_before[0].opacity,
        "A尚未消失，不能突然移除"
    );
    assert_eq!(
        transition.projects.as_ref().unwrap().layers[1].scale,
        layers_before[1].scale,
        "B局部尺寸应连续"
    );
    transition.retarget(subject("a"), full);
    transition.tick(Duration::from_millis(16));
    assert!(
        transition.camera.rect().origin.x < previous.origin.x,
        "反向首帧仍保留旧速度，不能瞬间折返"
    );
    for _ in 0..100 {
        transition.tick(Duration::from_millis(16));
    }
    assert_eq!(transition.rect(), full, "A应落到完整画布");
    assert!(!transition.is_active(), "落定后必须停止请求动画帧");
    let layers = &transition.projects.as_ref().unwrap().layers;
    assert_eq!(layers[0].opacity, 1.0, "A恢复完全可见");
    assert_eq!(layers[0].scale, 1.0, "A恢复原有卡片尺寸");
    assert_eq!(layers[1].opacity, 0.0, "B不可残留");
    assert_eq!(layers[2].opacity, 0.0, "C不可残留");
}
