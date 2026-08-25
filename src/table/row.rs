/// 一行原始数据。
///
/// `checked` 属于原始行而不是显示行，因此过滤、排序和插入都不会把勾选状态
/// 错配到另一行。UI 收到的 source_index 也始终来自过滤前的行号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<String>,
    pub checked: bool,
}

impl TableRow {
    pub fn new(cells: Vec<String>) -> Self {
        Self { cells, checked: false }
    }
}

/// 交给 Slint 展示的行快照。
///
/// 该结构明确分离“显示顺序”和“底层行号”：排序只改变快照顺序，
/// source_index 仍可直接索引原始 rows。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleTableRow {
    pub source_index: usize,
    pub cells: Vec<String>,
    pub checked: bool,
    pub selected: bool,
}
