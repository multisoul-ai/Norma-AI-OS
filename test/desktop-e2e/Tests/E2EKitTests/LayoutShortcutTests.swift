import Testing
@testable import E2EKit

struct LayoutShortcutTests {
    /// 场景：布局验收需要实际触发合法 Master 及手动重排快捷键。
    /// 构造：使用 macOS 的 M=46、L=37 与 Command/Shift 虚拟键码。
    /// 执行：经过生产 GPUI 按键编码和 AppKit 字符编码。
    /// 预期：两条输入路径都能表达原快捷键，不因驱动映射缺失退出 ENV。
    @Test func layoutShortcutsUseProductionKeyMappings() throws {
        #expect(try keystroke(key: 46, modifiers: [0x37]) == "cmd-m", "Master 快捷键必须保留 Command")
        #expect(try keystroke(key: 37, modifiers: [0x37, 0x38]) == "cmd-shift-l", "重排快捷键必须保留两个修饰键")
        #expect(try keyCharacters(46) == "m", "AppKit 必须收到 M 的字符")
        #expect(try keyCharacters(37) == "l", "AppKit 必须收到 L 的字符")
    }
}
