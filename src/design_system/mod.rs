// 设计系统对外只暴露稳定的主题、文本缩放、locale 和文案 catalog 类型。
pub mod i18n;
pub mod theme;

pub use i18n::{I18nCatalog, Locale, TextKey};
pub use theme::{EffectiveTheme, TextScale, ThemeMode};
