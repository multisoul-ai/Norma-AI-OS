//! Product language preferences and translation at the presentation boundary.
include!(concat!(env!("OUT_DIR"), "/i18n_keys.rs"));

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LanguagePreference {
    #[default]
    System,
    SimplifiedChinese,
    English,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    ZhCn,
    En,
}

pub struct LocaleState(pub Locale);
impl gpui::Global for LocaleState {}

pub fn set_app_locale(locale: Locale, cx: &mut gpui::App) {
    locale.apply();
    cx.set_global(LocaleState(locale));
    cx.refresh_windows();
}

/// Register a translated placeholder on a long-lived input, preserving its text and selection.
pub fn input_placeholder(
    key: &'static str,
    window: &gpui::Window,
    cx: &mut gpui::Context<crate::component::input::InputState>,
) -> &'static str {
    cx.observe_global_in::<LocaleState>(window, move |input, window, cx| {
        input.set_placeholder(text(key), window, cx);
    })
    .detach();
    text(key)
}

impl LanguagePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::SimplifiedChinese, Self::English];

    pub fn from_saved(value: Option<&str>) -> Self {
        match value {
            Some("zh-CN") => Self::SimplifiedChinese,
            Some("en") => Self::English,
            _ => Self::System,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::SimplifiedChinese => "zh-CN",
            Self::English => "en",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => text("general.follow_system"),
            Self::SimplifiedChinese => "简体中文",
            Self::English => "English",
        }
    }

    pub fn resolve(self, preferred: &[impl AsRef<str>]) -> Locale {
        match self {
            Self::SimplifiedChinese => Locale::ZhCn,
            Self::English => Locale::En,
            Self::System => preferred
                .iter()
                .find_map(|language| {
                    let language = language.as_ref().split(['-', '_']).next()?;
                    if language.eq_ignore_ascii_case("zh") {
                        Some(Locale::ZhCn)
                    } else if language.eq_ignore_ascii_case("en") {
                        Some(Locale::En)
                    } else {
                        None
                    }
                })
                .unwrap_or(Locale::En),
        }
    }
}

/// Resolve a static product label without allocating a new string on each frame.
pub fn text(key: &'static str) -> &'static str {
    text_for(Locale::current(), key)
}

pub fn text_for(locale: Locale, key: &'static str) -> &'static str {
    crate::_RUST_I18N_BACKEND
        .translate(locale.as_str(), key)
        .or_else(|| crate::_RUST_I18N_BACKEND.translate("en", key))
        .unwrap_or(key)
}

pub fn format(key: &'static str, arguments: &[(&'static str, String)]) -> String {
    message(key, arguments).to_string()
}

pub fn message(key: &'static str, arguments: &[(&'static str, String)]) -> LocalizedText {
    let mut value = LocalizedText::new(key);
    for (name, argument) in arguments {
        value = value.with_arg(name, argument);
    }
    value
}

/// Product messages retain semantic keys and raw arguments. Both projections are
/// prepared once; every read chooses the current locale without touching user text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    key: Option<&'static str>,
    arguments: Vec<(&'static str, String)>,
    rendered: [String; 2],
}

impl LocalizedText {
    pub fn new(key: &'static str) -> Self {
        let mut value = Self {
            key: Some(key),
            arguments: Vec::new(),
            rendered: Default::default(),
        };
        value.refresh();
        value
    }

    pub fn with_arg(mut self, name: &'static str, value: impl ToString) -> Self {
        self.arguments.push((name, value.to_string()));
        self.refresh();
        self
    }

    fn refresh(&mut self) {
        if let Some(key) = self.key {
            self.rendered = [Locale::En, Locale::ZhCn]
                .map(|locale| interpolate(text_for(locale, key), &self.arguments));
        }
    }

    pub fn resolve_for(&self, locale: Locale) -> String {
        self.rendered[usize::from(locale == Locale::ZhCn)].clone()
    }

    pub fn as_str(&self) -> &str {
        &self.rendered[usize::from(Locale::current() == Locale::ZhCn)]
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn interpolate(template: &str, arguments: &[(&'static str, String)]) -> String {
    let mut remaining = template;
    let mut output = String::with_capacity(template.len());
    while let Some(start) = remaining.find("%{") {
        let token = &remaining[start + 2..];
        let Some(end) = token.find('}') else { break };
        output.push_str(&remaining[..start]);
        let name = &token[..end];
        if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push_str(&remaining[start..start + end + 3]);
        }
        remaining = &token[end + 1..];
    }
    output.push_str(remaining);
    output
}

impl std::ops::Deref for LocalizedText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for LocalizedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for LocalizedText {
    fn default() -> Self {
        String::new().into()
    }
}

impl From<String> for LocalizedText {
    fn from(value: String) -> Self {
        Self {
            key: None,
            arguments: Vec::new(),
            rendered: [value.clone(), value],
        }
    }
}

impl From<&str> for LocalizedText {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

impl Locale {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    pub fn current() -> Self {
        if &*rust_i18n::locale() == "zh-CN" {
            Self::ZhCn
        } else {
            Self::En
        }
    }

    pub fn apply(self) {
        crate::component::set_locale(self.as_str());
    }
}

impl gpui::IntoElement for LocalizedText {
    type Element = <String as gpui::IntoElement>::Element;
    fn into_element(self) -> Self::Element {
        self.to_string().into_element()
    }
}
