# Test Requirements

All unit tests should follow the project style already present in the crate tests.

## Crate-local test layout

- Large standalone unit test files live under `crates/<crate>/tests/unit/` and are mounted from the owning source module with `#[cfg(test)] #[path = "..."] mod ...;`.
- Cargo integration tests live directly under `crates/<crate>/tests/*.rs` and should exercise public APIs only.
- Crate-specific test helpers live under `crates/<crate>/tests/support/` when needed.
- Do not make private functions or types public just to move a test file.

## Doc comment structure

Each test needs a doc comment that states:

- Scenario: one sentence describing the behavior under test.
- Data construction: include important concrete values and formulas.
- Execution process: step-by-step behavior and state changes.
- Expected result: positive and negative acceptance criteria.

## Assertions

- Every `assert!`, `assert_eq!`, and `assert_ne!` must include a failure message.
- Prefer specific assertions for named states over generic loops.
- Cover both what should exist and what should not exist when that distinction matters.
- Boundary numbers such as token budgets, timeouts, viewport transforms, rows/cols, and byte values need comments explaining the calculation.

## Example pattern

```rust
/// Ctrl+C 编码为 ETX(0x03)。
///
/// 数值推导：control code = ascii(letter) & 0x1f
///   'c' = 0x63；0x63 & 0x1f = 0x03
///
/// 预期结果：
///   - 正断言：encode(Ctrl+'c') == [0x03]
///   - 负断言：!= [0x63]（不是裸字符 'c'）
#[test]
fn test_ctrl_c_encodes_to_etx() {
    let out = encode(&KeyInput { key: Key::Char('c'), ctrl: true, alt: false });
    assert_eq!(out, vec![0x03], "Ctrl+C 应编码为 0x03（'c'&0x1f）");
    assert_ne!(out, vec![0x63], "不应是裸 'c'(0x63)，说明 ctrl 修饰未生效");
}
```
