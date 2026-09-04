use super::locale::Locale;
use super::types::TextKey;

pub(super) const fn text(locale: Locale, key: TextKey) -> &'static str {
    match (locale, key) {
        (Locale::ZhCn, TextKey::TableResetWidths) => "恢复默认列宽",
        (Locale::ZhCn, TextKey::TableResetTitles) => "显示全部列",
        (Locale::ZhCn, TextKey::TableShowColumns) => "显示列",
        (Locale::EnUs, TextKey::TableResetWidths) => "Reset column widths",
        (Locale::EnUs, TextKey::TableResetTitles) => "Show all columns",
        (Locale::EnUs, TextKey::TableShowColumns) => "Show columns",
        _ => "",
    }
}
