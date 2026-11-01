use ui::i18n::Locale;

/// 测试期间固定界面语言为简体中文，Drop 时恢复。
///
/// 默认语言取自 macOS 系统首选语言：首选为英文的开发机上，断言中文文案的测试会失败。
/// 语言是进程级全局状态，用本守卫显式固定，避免测试结果依赖运行机器的系统语言。
pub(crate) struct PinnedLocale(Locale);

impl PinnedLocale {
    pub(crate) fn zh_cn() -> Self {
        let previous = Locale::current();
        Locale::ZhCn.apply();
        Self(previous)
    }
}

impl Drop for PinnedLocale {
    fn drop(&mut self) {
        self.0.apply();
    }
}
