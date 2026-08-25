/// 表格列的领域描述。
///
/// `title` 只负责展示；`sortable` 决定表头是否可以触发字典序排序。
/// `width_weight` 是相对宽度权重，不参与数据逻辑。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    pub title: String,
    pub sortable: bool,
    pub width_weight: u32,
}

impl TableColumn {
    pub fn new(title: impl Into<String>, sortable: bool, width_weight: u32) -> Self {
        Self {
            title: title.into(),
            sortable,
            width_weight: width_weight.max(1),
        }
    }
}
