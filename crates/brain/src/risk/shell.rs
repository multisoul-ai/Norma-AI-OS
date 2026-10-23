//! 极简 shell 拆分：只识别分隔符、管道、引号、重定向。
//! 遇到命令替换、子 shell、heredoc 等无法静态判断的语法，返回 `None`。

#[derive(Debug, Default)]
pub(super) struct Segment {
    pub argv: Vec<String>,
    pub redirects: Vec<String>,
    /// 标准输入来自上一段的管道。
    pub piped: bool,
}

pub(super) fn parse(command: &str) -> Option<Vec<Segment>> {
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    let mut segments = Vec::new();
    let mut current = Segment::default();
    let mut next_piped = false;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => i += 1,
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\n' | ';' => {
                end_segment(&mut segments, &mut current, false, &mut next_piped);
                i += 1;
            }
            '|' => {
                let double = chars.get(i + 1) == Some(&'|');
                i += if double { 2 } else { 1 };
                end_segment(&mut segments, &mut current, !double, &mut next_piped);
            }
            '&' => match chars.get(i + 1) {
                Some('>') => i += 1,
                Some('&') => {
                    i += 2;
                    end_segment(&mut segments, &mut current, false, &mut next_piped);
                }
                _ => {
                    i += 1;
                    end_segment(&mut segments, &mut current, false, &mut next_piped);
                }
            },
            '>' => {
                if i > 0 && !chars[i - 1].is_whitespace() && is_fd(current.argv.last()) {
                    current.argv.pop();
                }
                i += 1;
                if chars.get(i) == Some(&'>') {
                    i += 1;
                }
                if chars.get(i) == Some(&'&') {
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '-') {
                        i += 1;
                    }
                    continue;
                }
                skip_blanks(&chars, &mut i);
                current.redirects.push(read_word(&chars, &mut i)?);
            }
            '<' => {
                i += 1;
                if matches!(chars.get(i), Some('<') | Some('(')) {
                    return None;
                }
                skip_blanks(&chars, &mut i);
                current.argv.push(read_word(&chars, &mut i)?);
            }
            '(' | ')' => return None,
            _ => {
                let word = read_word(&chars, &mut i)?;
                if current.argv.is_empty() && word.starts_with('$') {
                    return None;
                }
                if current.argv.is_empty() {
                    current.piped = std::mem::take(&mut next_piped);
                }
                current.argv.push(word);
            }
        }
    }
    end_segment(&mut segments, &mut current, false, &mut next_piped);
    Some(segments)
}

fn is_fd(word: Option<&String>) -> bool {
    word.is_some_and(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
}

fn skip_blanks(chars: &[char], i: &mut usize) {
    while matches!(chars.get(*i), Some(' ') | Some('\t')) {
        *i += 1;
    }
}

fn end_segment(
    segments: &mut Vec<Segment>,
    current: &mut Segment,
    pipe_next: bool,
    next_piped: &mut bool,
) {
    if !current.argv.is_empty() || !current.redirects.is_empty() {
        segments.push(std::mem::take(current));
    }
    *next_piped = pipe_next;
}

/// 读一个 shell 词；未加引号遇到空白或运算符时结束。
fn read_word(chars: &[char], i: &mut usize) -> Option<String> {
    let mut word = String::new();
    while let Some(&c) = chars.get(*i) {
        match c {
            ' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' => break,
            '(' | ')' | '`' => return None,
            '$' if chars.get(*i + 1) == Some(&'(') => return None,
            '\\' => {
                word.push(*chars.get(*i + 1)?);
                *i += 2;
            }
            '\'' => {
                *i += 1;
                while *chars.get(*i)? != '\'' {
                    word.push(chars[*i]);
                    *i += 1;
                }
                *i += 1;
            }
            '"' => {
                *i += 1;
                loop {
                    match *chars.get(*i)? {
                        '"' => break,
                        '`' => return None,
                        '$' if chars.get(*i + 1) == Some(&'(') => return None,
                        '\\' => {
                            word.push(*chars.get(*i + 1)?);
                            *i += 1;
                        }
                        other => word.push(other),
                    }
                    *i += 1;
                }
                *i += 1;
            }
            other => {
                word.push(other);
                *i += 1;
            }
        }
    }
    Some(word)
}
