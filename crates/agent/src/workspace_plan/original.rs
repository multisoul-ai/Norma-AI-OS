use norma::workspace_dispatch::*;

/// Deliberately conservative recipient grammar, not a general NLP parser. Unknown
/// boundaries, indirect references and unsupported phrasing require target completion.
pub fn build_original_target_manifest(
    directory: &WorkspaceDirectory,
    mode: WorkspaceRoutingMode,
    body: &str,
    mentions: &[WorkspaceStructuredMention],
) -> OriginalTargetManifest {
    let mut result = OriginalTargetManifest {
        original_body: body.into(),
        mode,
        complete: true,
        targets: vec![],
        evidence: vec![],
    };
    if mode != WorkspaceRoutingMode::Execute {
        return result;
    }
    let (mut masked, balanced) = mask_quoted(body);
    result.complete = balanced;
    let mut previous_end = 0;
    let mut ordered = mentions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|m| m.start_byte);
    for mention in ordered {
        let literal = body.get(mention.start_byte..mention.end_byte);
        let valid = mention.start_byte >= previous_end
            && literal.is_some_and(|s| s.starts_with('@') && s.len() > 1)
            && masked.get(mention.start_byte..mention.end_byte) == literal
            && mention.card.validate().is_ok()
            && mention.card.workspace_id == directory.workspace_id;
        if !valid {
            result.complete = false;
            continue;
        }
        add(
            &mut result,
            body,
            mention.start_byte,
            mention.end_byte,
            WorkspaceTargetIntent::Card {
                card: mention.card.clone(),
                project: mention.project.clone(),
            },
        );
        mask_range(&mut masked, mention.start_byte, mention.end_byte);
        previous_end = mention.end_byte;
    }
    let mut index = 0;
    while index < masked.len() {
        let rest = &masked[index..];
        let ch = rest.chars().next().unwrap_or(' ');
        let previous = masked[..index].chars().next_back();
        let boundary = previous.is_none_or(|c| c.is_whitespace() || is_separator(c));
        let marker = if ch == '@' {
            Some(1)
        } else if boundary
            || ["同时", "并", "然后"]
                .iter()
                .any(|p| masked[..index].ends_with(p))
        {
            ["请让", "请给", "让", "给", "请", "ask ", "tell ", "assign "]
                .iter()
                .find_map(|m| prefix(rest, m).map(|_| m.len()))
        } else {
            None
        };
        let known_leading = boundary
            && (index == 0 || previous.is_some_and(is_separator))
            && (known_name(directory, rest).is_some()
                || index > 0
                    && !looks_like_task(rest)
                    && explicit_token(rest).is_some_and(|(name, _)| name.is_ascii()));
        if marker.is_some() || known_leading {
            let start = skip_space(&masked, index + marker.unwrap_or(0));
            match recipient_list(directory, body, &masked, start, &mut result) {
                Some(end) => {
                    index = end;
                    continue;
                }
                None => result.complete = false,
            }
        }
        index += ch.len_utf8();
    }
    // No proved recipient is not permission for the model to choose one by ability/focus.
    if result.targets.is_empty() {
        result.complete = false;
    }
    if let Some(first) = result.evidence.iter().map(|e| e.start_byte).min() {
        // Source text can veto an Execute classification supplied by a model.
        // Task content after the first recipient is deliberately not classified here.
        let before = masked[..first].to_ascii_lowercase();
        if [
            "讨论",
            "假设",
            "如果",
            "怎么",
            "如何",
            "例如",
            "举例",
            "是否",
            "要不要",
            "想了解",
            "先聊",
            "商量",
            "不要",
            "discuss",
            "hypothetical",
            "how to",
            "what if",
            "example",
            "do not",
            "don't",
            "never",
        ]
        .iter()
        .any(|marker| before.contains(marker))
        {
            result.complete = false;
        }
    }
    // A role mention can also precede a discussion/question. Only inspect the
    // beginning after each proved recipient, so quoted task data stays untouched.
    if result.evidence.iter().any(|e| {
        let after =
            masked[e.end_byte..].trim_start_matches(|c: char| c.is_whitespace() || is_separator(c));
        [
            "我们讨论",
            "讨论",
            "如何",
            "怎么",
            "假设",
            "如果",
            "不要",
            "let's discuss",
            "discuss ",
            "how to ",
            "what if ",
            "do not ",
        ]
        .iter()
        .any(|marker| prefix(after, marker).is_some())
    }) {
        result.complete = false;
    }
    // Addressing a role is weaker than an imperative. A bare/structured @ mention
    // must also have a proved command prefix; unsupported phrasing stays editable.
    if result.evidence.iter().any(|e| {
        let before = masked[..e.start_byte].trim_end();
        let imperative = ["让", "给", "请", "ask", "tell", "assign"]
            .iter()
            .any(|marker| before.ends_with(marker));
        let after =
            masked[e.end_byte..].trim_start_matches(|c: char| c.is_whitespace() || is_separator(c));
        let action = ["分别", "各自", "一起"]
            .iter()
            .find_map(|modifier| prefix(after, modifier))
            .unwrap_or(after)
            .trim_start();
        !imperative && !looks_like_task(action)
    }) {
        result.complete = false;
    }
    result
}

fn recipient_list(
    directory: &WorkspaceDirectory,
    body: &str,
    masked: &str,
    mut start: usize,
    result: &mut OriginalTargetManifest,
) -> Option<usize> {
    let project_start = start;
    let mut project_name = None;
    let rest = &masked[start..];
    // Explicit "Web 项目的 Alan" / "Web 项目" preserves the project constraint.
    if let Some(pos) = rest.find("项目") {
        let before = rest[..pos].trim();
        if !before.is_empty()
            && !before.chars().any(is_separator)
            && before.split_whitespace().count() == 1
        {
            let project_end = start + pos + "项目".len();
            let following = &masked[project_end..];
            if let Some(after) = following.strip_prefix('的') {
                project_name = Some(before.to_owned());
                start = masked.len() - after.len();
                start = skip_space(masked, start);
            } else {
                add(
                    result,
                    body,
                    start,
                    project_end,
                    WorkspaceTargetIntent::ProjectName {
                        name: before.to_owned(),
                    },
                );
                return Some(project_end);
            }
        }
    }
    loop {
        let rest = &masked[start..];
        let (name, bytes) = known_name(directory, rest).or_else(|| explicit_token(rest))?;
        let target = if let Some(project_name) = &project_name {
            WorkspaceTargetIntent::CallNameInProject {
                name: name.clone(),
                project_name: project_name.clone(),
            }
        } else {
            WorkspaceTargetIntent::CallName {
                name: name.clone(),
                project: None,
            }
        };
        add(
            result,
            body,
            if project_name.is_some() {
                project_start
            } else {
                start
            },
            start + bytes,
            target,
        );
        let end = start + bytes;
        let after_space = skip_space(masked, end);
        let rest = &masked[after_space..];
        let conjunction = ["以及", "还有", "和", "与", "、", "and ", "&", ",", "，"]
            .iter()
            .find_map(|c| {
                prefix(rest, c)
                    .filter(|after| {
                        !matches!(*c, "," | "，") || !looks_like_task(after.trim_start())
                    })
                    .map(|_| c.len())
            });
        if let Some(length) = conjunction {
            start = skip_space(masked, after_space + length);
            if start >= masked.len() {
                result.complete = false;
                return Some(end);
            }
        } else {
            return Some(end);
        }
    }
}

fn known_name(directory: &WorkspaceDirectory, input: &str) -> Option<(String, usize)> {
    directory
        .call_keys
        .iter()
        .filter_map(|key| {
            prefix(input, &key.key)?;
            let end = key.key.len();
            let next = input[end..].chars().next();
            if next.is_some_and(|c| c.is_ascii_alphabetic()) {
                return None;
            }
            // Chinese names without a separator are accepted only before a known action.
            if next.is_some_and(|c| !c.is_whitespace() && !is_separator(c) && !c.is_ascii())
                && ![
                    "修复", "检查", "继续", "完成", "实现", "处理", "写", "做", "按", "帮", "请",
                ]
                .iter()
                .any(|v| input[end..].starts_with(v))
            {
                return None;
            }
            Some((input[..end].to_owned(), end))
        })
        .max_by_key(|(_, bytes)| *bytes)
}

fn explicit_token(input: &str) -> Option<(String, usize)> {
    let ascii_end = input
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map(|(i, _)| i)
        .unwrap_or(input.len());
    if (2..=16).contains(&ascii_end) && looks_like_task(&input[ascii_end..]) {
        return Some((input[..ascii_end].into(), ascii_end));
    }
    let end = input
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || is_separator(*c))
        .map(|(i, _)| i)?;
    let token = &input[..end];
    let english = (2..=16).contains(&token.len()) && token.chars().all(|c| c.is_ascii_alphabetic());
    let chinese = (2..=8).contains(&token.chars().count())
        && token
            .chars()
            .all(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'));
    (english || chinese).then(|| (token.into(), end))
}
fn looks_like_task(input: &str) -> bool {
    [
        "修复",
        "检查",
        "继续",
        "完成",
        "实现",
        "处理",
        "写",
        "做",
        "按",
        "帮",
        "请",
        "fix ",
        "check ",
        "review ",
        "implement ",
        "run ",
        "test ",
        "verify ",
        "write ",
    ]
    .iter()
    .any(|action| prefix(input, action).is_some())
}
fn prefix<'a>(input: &'a str, value: &str) -> Option<&'a str> {
    input
        .get(..value.len())
        .filter(|s| s.eq_ignore_ascii_case(value))
        .map(|_| &input[value.len()..])
}
fn skip_space(input: &str, index: usize) -> usize {
    index + input[index..].len() - input[index..].trim_start().len()
}
fn is_separator(c: char) -> bool {
    matches!(
        c,
        ',' | '，' | ':' | '：' | ';' | '；' | '、' | '.' | '。' | '\n' | '(' | '（'
    )
}
fn add(
    result: &mut OriginalTargetManifest,
    body: &str,
    start: usize,
    end: usize,
    target: WorkspaceTargetIntent,
) {
    if !result.targets.contains(&target) {
        result.targets.push(target.clone());
    }
    result.evidence.push(WorkspaceOriginalTargetEvidence {
        start_byte: start,
        end_byte: end,
        literal: body[start..end].into(),
        target,
    });
}
fn mask_range(input: &mut String, start: usize, end: usize) {
    let mut bytes = input.as_bytes().to_vec();
    bytes[start..end].fill(b' ');
    *input = String::from_utf8(bytes).expect("masked ranges cover full UTF-8 characters");
}
fn mask_quoted(body: &str) -> (String, bool) {
    let mut bytes = body.as_bytes().to_vec();
    let mut closer = None;
    for (start, c) in body.char_indices() {
        if let Some(close) = closer {
            bytes[start..start + c.len_utf8()].fill(b' ');
            if c == close {
                closer = None;
            }
        } else if let Some(close) = match c {
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            '“' => Some('”'),
            '‘' => Some('’'),
            '「' => Some('」'),
            _ => None,
        } {
            closer = Some(close);
            bytes[start..start + c.len_utf8()].fill(b' ');
        }
    }
    (
        String::from_utf8(bytes).expect("full quote characters are masked"),
        closer.is_none(),
    )
}
