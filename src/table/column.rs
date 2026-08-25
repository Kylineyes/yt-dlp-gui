/// 表格列的领域描述。
///
/// `title` 只负责展示；`sortable` 决定表头是否可以触发字典序排序。
/// `font_family` 为空时，Slint 组件使用主题的 UI 字体；非空时使用调用方指定的字体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    pub title: String,
    pub sortable: bool,
    pub width_weight: u32,
    pub font_family: String,
}

impl TableColumn {
    pub fn new(title: impl Into<String>, sortable: bool, width_weight: u32) -> Self {
        Self {
            title: title.into(),
            sortable,
            width_weight: width_weight.max(1),
            font_family: String::new(),
        }
    }

    /// 为该列设置字体族，例如主题提供的等宽字体。
    pub fn font_family(mut self, font_family: impl Into<String>) -> Self {
        self.font_family = font_family.into();
        self
    }
}
