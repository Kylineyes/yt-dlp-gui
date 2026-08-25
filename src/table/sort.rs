/// 显示行的排序策略。
///
/// `Preordered` 是稳定的预排序契约：控制器只做过滤和快照映射，绝不对行再次排序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSortStrategy {
    Dictionary,
    Preordered,
}
/// 表格排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSortDirection {
    Unsorted,
    Ascending,
    Descending,
}

/// 当前排序状态，同时保留给 UI 生成排序图标使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSort {
    pub column: usize,
    pub direction: TableSortDirection,
}

/// 以不区分大小写的方式比较两个文本单元格。
///
/// 表格本身只实现通用字典序；如果业务需要数值、时间或自定义顺序，
/// 应在把数据转换为表格行之前完成对应的业务排序。
pub fn compare_lexicographic(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}
