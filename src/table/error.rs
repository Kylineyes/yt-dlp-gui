/// 表格数据操作失败时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    RowWidthMismatch { expected: usize, actual: usize },
    ColumnOutOfBounds(usize),
    RowOutOfBounds(usize),
    ColumnNotSortable(usize),
}
