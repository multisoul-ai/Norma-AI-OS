/// Select an explicit expected literal for tests that share the app's active locale.
/// Product translation keys are deliberately not consulted by this test oracle.
pub fn expected_copy<'a>(chinese: &'a str, english: &'a str) -> &'a str {
    match ui::i18n::Locale::current() {
        ui::i18n::Locale::ZhCn => chinese,
        ui::i18n::Locale::En => english,
    }
}
