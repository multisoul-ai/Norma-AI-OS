//! Extract explicit facts and complete steps from a staff task without rewriting its payload.

/// Only actual, complete bullet lines are projected. Long instructions remain in the original.
pub fn task_points(task: &str) -> Vec<&str> {
    task.lines()
        .filter_map(|line| {
            let line = line.trim();
            let digits = line.bytes().take_while(u8::is_ascii_digit).count();
            let numbered = (digits > 0).then(|| &line[digits..]);
            let text = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("• "))
                .or_else(|| {
                    numbered.and_then(|s| s.strip_prefix(". ").or_else(|| s.strip_prefix(") ")))
                })?
                .trim();
            // At most three short lines keep the inline summary bounded without slicing a constraint.
            (!text.is_empty() && text.chars().count() <= 80).then_some(text)
        })
        .take(3)
        .collect()
}

pub struct TaskSummary<'a> {
    pub goal: Option<&'a str>,
    pub steps: Vec<&'a str>,
    pub output: Option<&'a str>,
}

/// Project explicit facts only; never invent a summary or replace the dispatch payload.
pub fn parse(task: &str) -> TaskSummary<'_> {
    let fact = |labels: &[&str]| {
        task.lines().find_map(|line| {
            let line = line.trim().trim_start_matches('#').trim();
            labels.iter().find_map(|label| {
                line.strip_prefix(label)
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && value.chars().count() <= 100)
            })
        })
    };
    let steps = task_points(task);
    let goal = fact(&["目标：", "目标:", "Goal:", "Objective:"]).or_else(|| {
        let text = task.trim();
        (steps.is_empty()
            && !text.contains('\n')
            && !text.is_empty()
            && text.chars().count() <= 100)
            .then_some(text)
    });
    TaskSummary {
        goal,
        steps,
        output: fact(&[
            "验收：",
            "验收:",
            "交付：",
            "交付:",
            "Acceptance:",
            "Output:",
            "Done when:",
        ]),
    }
}

#[cfg(test)]
#[path = "../tests/unit/task_outline_tests.rs"]
mod tests;
