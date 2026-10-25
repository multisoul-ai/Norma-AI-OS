//! 只接受一条字面量命令；任何未加引号的 shell 运算符都失败关闭。

pub(super) fn parse(command: &str) -> Option<Vec<String>> {
    if command.contains(['\0', '\r']) || command.contains("\\\n") {
        return None;
    }
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    let mut argv = Vec::new();
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => i += 1,
            '\n' | ';' | '|' | '&' | '<' | '>' | '(' | ')' | '#' => return None,
            _ => {
                let word = read_word(&chars, &mut i)?;
                if argv.is_empty() && word.starts_with('$') {
                    return None;
                }
                argv.push(word);
            }
        }
    }
    (!argv.is_empty()).then_some(argv)
}

/// 读一个 shell 词；未加引号遇到空白或运算符时结束。
fn read_word(chars: &[char], i: &mut usize) -> Option<String> {
    let mut word = String::new();
    while let Some(&c) = chars.get(*i) {
        match c {
            ' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' => break,
            '(' | ')' | '`' | '$' => return None,
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
                        '`' | '$' => return None,
                        '\\' => {
                            let next = *chars.get(*i + 1)?;
                            // 双引号里只有这四个字符可由反斜杠转义；不能把 \q 变成 q。
                            if matches!(next, '\\' | '"' | '$' | '`') {
                                word.push(next);
                                *i += 1;
                            } else {
                                word.push('\\');
                            }
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
