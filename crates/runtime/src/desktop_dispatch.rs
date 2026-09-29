#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopAgentTarget {
    pub project_name: String,
    pub agent_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopDispatch {
    pub target_index: usize,
    pub instruction: String,
}

pub fn route_desktop_command(
    input: &str,
    targets: &[DesktopAgentTarget],
) -> Result<Vec<DesktopDispatch>, String> {
    if input.trim().is_empty() {
        return Err("请输入指令".into());
    }
    if targets.is_empty() {
        return Err("当前工作空间没有可派工的 Agent".into());
    }
    let projects: Vec<_> = targets
        .iter()
        .map(|target| target.project_name.as_str())
        .fold(Vec::<&str>::new(), |mut names, name| {
            if !names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                names.push(name);
            }
            names
        });
    let mut dispatches = Vec::new();
    for clause in input.replace("同时", ";").split([';', '；', '\n']) {
        let clause = clause.trim_matches(|character: char| {
            character.is_whitespace() || character == ',' || character == '，'
        });
        if clause.is_empty() {
            continue;
        }
        let mentioned: Vec<_> = projects
            .iter()
            .copied()
            .filter(|name| contains_name(clause, name))
            .collect();
        let project = match mentioned.as_slice() {
            [name] => *name,
            [] if projects.len() == 1
                && !clause.contains("项目 ")
                && !clause.to_lowercase().contains("project ") =>
            {
                projects[0]
            }
            _ => {
                return Err(format!(
                    "请明确指定当前工作空间中的项目：{}",
                    projects.join("、")
                ))
            }
        };
        let candidates: Vec<_> = targets
            .iter()
            .enumerate()
            .filter(|(_, target)| target.project_name.eq_ignore_ascii_case(project))
            .collect();
        let agents: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|(_, target)| contains_name(clause, &target.agent_name))
            .collect();
        let target_index = match agents.as_slice() {
            [(index, _)] => *index,
            [] if candidates.len() == 1 => candidates[0].0,
            _ => {
                return Err(format!(
                    "请明确指定 {project} 的 Agent：{}",
                    candidates
                        .iter()
                        .map(|(_, target)| target.agent_name.as_str())
                        .collect::<Vec<_>>()
                        .join("、")
                ))
            }
        };
        dispatches.push(DesktopDispatch {
            target_index,
            instruction: clause.to_string(),
        });
    }
    if dispatches.is_empty() {
        return Err("请输入指令".into());
    }
    Ok(dispatches)
}

fn contains_name(text: &str, name: &str) -> bool {
    let text = text.to_lowercase();
    let name = name.to_lowercase();
    if name.is_empty() {
        return false;
    }
    text.match_indices(&name).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + name.len()..].chars().next();
        let left_ok = !name.starts_with(|character: char| character.is_ascii_alphanumeric())
            || before.is_none_or(|character| !character.is_ascii_alphanumeric());
        let right_ok = !name.ends_with(|character: char| character.is_ascii_alphanumeric())
            || after.is_none_or(|character| !character.is_ascii_alphanumeric());
        left_ok && right_ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DWS-06：一条明确指向两个项目的指令应分别派发给各自 Agent。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   当前空间有项目 A/B 各 1 张 Agent 卡片；两个明确子句应产生 1 + 1 = 2 个派发。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析「同时」分隔的两个子句。
    ///   2. 用项目和 Agent 的精确名称匹配当前空间的目标。
    ///
    /// 预期结果：
    ///   - 正断言：A 和 B 分别收到自己的子句。
    ///   - 负断言：A 的任务不包含 B 的指令，B 的任务不包含 A 的指令。
    #[test]
    fn routes_two_named_projects_independently() {
        let targets = vec![
            DesktopAgentTarget {
                project_name: "A".into(),
                agent_name: "登录".into(),
            },
            DesktopAgentTarget {
                project_name: "B".into(),
                agent_name: "测试".into(),
            },
        ];
        let routed = route_desktop_command(
            "让项目 A 的登录 Agent 修复问题，同时让项目 B 的测试 Agent 跑回归",
            &targets,
        )
        .expect("两个项目和 Agent 都明确时应可派发");
        assert_eq!(routed.len(), 2, "一条跨项目指令应生成两个独立派发");
        assert_eq!(routed[0].target_index, 0, "首个子句应派给 A/登录");
        assert!(
            routed[0].instruction.contains("修复问题"),
            "A 应收到修复任务"
        );
        assert!(
            !routed[0].instruction.contains("跑回归"),
            "A 不应收到 B 的任务"
        );
        assert_eq!(routed[1].target_index, 1, "第二个子句应派给 B/测试");
        assert!(routed[1].instruction.contains("跑回归"), "B 应收到回归任务");
        assert!(
            !routed[1].instruction.contains("修复问题"),
            "B 不应收到 A 的任务"
        );
    }

    /// DWS-07：多个项目可选但没有明确项目时先澄清，不执行派发。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   当前空间有 2 个项目、2 张 Agent 卡片；未指定项目时可选目标数 = 2 > 1。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析未指明项目的「修一下」。
    ///   2. 解析明确指向空间外「项目 C」的另一条指令。
    ///
    /// 预期结果：
    ///   - 正断言：两次都返回需要澄清的错误。
    ///   - 负断言：任何一次都不得产生目标派发。
    #[test]
    fn ambiguous_or_unknown_project_requires_clarification() {
        let targets = vec![
            DesktopAgentTarget {
                project_name: "A".into(),
                agent_name: "登录".into(),
            },
            DesktopAgentTarget {
                project_name: "B".into(),
                agent_name: "测试".into(),
            },
        ];
        assert!(
            route_desktop_command("修一下", &targets).is_err(),
            "多项目未指明目标必须澄清"
        );
        assert!(
            route_desktop_command("让项目 C 修一下", &targets).is_err(),
            "空间外项目不得隐式路由到当前空间"
        );
    }

    /// DWS-07：同一项目有两张 Agent 时只凭项目名不能猜卡片。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   项目 A 有登录、测试两张卡片；仅指定项目时可选 Agent 数 = 2 > 1。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析只指定 A 的指令 → 要求补充 Agent。
    ///   2. 解析同时指定 A 和测试 Agent 的指令 → 只选测试卡片。
    ///
    /// 预期结果：
    ///   - 正断言：明确 Agent 时目标为测试卡片。
    ///   - 负断言：含糊指令不派给登录卡片，也不生成任何派发。
    #[test]
    fn multiple_agents_in_one_project_require_exact_agent() {
        let targets = vec![
            DesktopAgentTarget {
                project_name: "A".into(),
                agent_name: "登录".into(),
            },
            DesktopAgentTarget {
                project_name: "A".into(),
                agent_name: "测试".into(),
            },
        ];
        assert!(
            route_desktop_command("让项目 A 修一下", &targets).is_err(),
            "两张 Agent 可选时必须澄清"
        );
        let routed = route_desktop_command("让项目 A 的测试 Agent 跑回归", &targets)
            .expect("明确 Agent 后应可派发");
        assert_eq!(routed.len(), 1, "明确目标只应派发一次");
        assert_eq!(routed[0].target_index, 1, "任务只能进入测试卡片");
        assert_ne!(routed[0].target_index, 0, "不能误选登录卡片");
    }
}
