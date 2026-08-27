use super::filter::matches_value;
use super::{
    compare_lexicographic, FilterSelection, TableColumn, TableError, TableFilter, TableRow, TableSort,
    TableSortDirection, TableSortStrategy, VisibleTableRow,
};

/// 通用表格的数据控制器。
///
/// 组件本身不猜测业务对象的字段含义；使用方把对象转换为字符串单元格后交给
/// 该控制器。控制器负责插入、过滤、排序、选中和勾选，并把可显示快照提供给
/// Slint 组件。这样 Search 页面可以继续使用自己的既有模型，不需要被改写。
#[derive(Debug, Clone)]
pub struct TableModel {
    columns: Vec<TableColumn>,
    rows: Vec<TableRow>,
    filters: Vec<TableFilter>,
    sort: Option<TableSort>,
    sort_strategy: TableSortStrategy,
    selected_source_row: Option<usize>,
}

impl TableModel {
    pub fn new(columns: Vec<TableColumn>, rows: Vec<TableRow>) -> Result<Self, TableError> {
        Self::validate_rows(columns.len(), &rows)?;
        Ok(Self {
            columns,
            rows,
            filters: Vec::new(),
            sort: None,
            sort_strategy: TableSortStrategy::Dictionary,
            selected_source_row: None,
        })
    }

    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }

    pub fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    pub fn filters(&self) -> &[TableFilter] {
        &self.filters
    }

    pub fn sort(&self) -> Option<TableSort> {
        self.sort
    }

    pub fn sort_strategy(&self) -> TableSortStrategy {
        self.sort_strategy
    }

    /// 设置显示顺序策略。切换策略时清除旧排序状态，避免图标和实际顺序不一致。
    pub fn set_sort_strategy(&mut self, strategy: TableSortStrategy) {
        self.sort_strategy = strategy;
        self.sort = None;
    }

    /// 用消费者已经排好顺序的结果替换原始行，并承诺不再进行二次排序。
    pub fn replace_rows_preordered(&mut self, rows: Vec<TableRow>) -> Result<(), TableError> {
        Self::validate_rows(self.columns.len(), &rows)?;
        self.set_sort_strategy(TableSortStrategy::Preordered);
        self.rows = rows;
        self.selected_source_row = None;
        Ok(())
    }

    /// 替换整个原始结果集合。替换会清除选中行，但保留过滤条件和排序策略。
    pub fn replace_rows(&mut self, rows: Vec<TableRow>) -> Result<(), TableError> {
        Self::validate_rows(self.columns.len(), &rows)?;
        self.rows = rows;
        self.selected_source_row = None;
        Ok(())
    }

    /// `set_rows` 是 `replace_rows` 的语义别名，便于页面模型按常见命名调用。
    pub fn set_rows(&mut self, rows: Vec<TableRow>) -> Result<(), TableError> {
        self.replace_rows(rows)
    }

    pub fn selected_source_row(&self) -> Option<usize> {
        self.selected_source_row
    }

    /// 返回当前过滤结果对应的原始行索引，顺序与 `visible_rows()` 一致。
    pub fn visible_source_indices(&self) -> Vec<usize> {
        self.filtered_indices()
    }

    /// 当前展示结果全部勾选时返回 true；空展示结果不视为全选。
    pub fn all_visible_checked(&self) -> bool {
        let indices = self.visible_source_indices();
        !indices.is_empty() && indices.iter().all(|&index| self.rows[index].checked)
    }

    /// 只修改当前展示行的勾选状态，不影响被过滤隐藏的原始行。
    pub fn set_all_visible_checked(&mut self, checked: bool) {
        for index in self.visible_source_indices() {
            self.rows[index].checked = checked;
        }
    }

    /// 返回当前过滤后的原始行索引，供显示快照和批量操作共享同一过滤契约。
    fn filtered_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.matches_filters(row))
            .map(|(index, _)| index)
            .collect();

        if self.sort_strategy == TableSortStrategy::Dictionary {
            if let Some(TableSort { column, direction }) = self.sort {
                indices.sort_by(|&left, &right| {
                    let ordering =
                        compare_lexicographic(&self.rows[left].cells[column], &self.rows[right].cells[column]);
                    if direction == TableSortDirection::Descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                    .then_with(|| left.cmp(&right))
                });
            }
        }

        indices
    }

    /// 在过滤前的原始行序列中插入一行。
    pub fn insert_row(&mut self, index: usize, row: TableRow) -> Result<(), TableError> {
        if row.cells.len() != self.columns.len() {
            return Err(TableError::RowWidthMismatch {
                expected: self.columns.len(),
                actual: row.cells.len(),
            });
        }
        if index > self.rows.len() {
            return Err(TableError::RowOutOfBounds(index));
        }
        self.rows.insert(index, row);
        if let Some(selected) = self.selected_source_row.as_mut() {
            if *selected >= index {
                *selected += 1;
            }
        }
        Ok(())
    }

    pub fn push_row(&mut self, row: TableRow) -> Result<(), TableError> {
        self.insert_row(self.rows.len(), row)
    }

    pub fn set_filters(&mut self, filters: Vec<TableFilter>) -> Result<(), TableError> {
        for filter in &filters {
            if let Some(column) = filter.column {
                if column >= self.columns.len() {
                    return Err(TableError::ColumnOutOfBounds(column));
                }
            }
        }
        self.filters = filters;
        Ok(())
    }

    pub fn clear_filters(&mut self) {
        self.filters.clear();
    }

    /// 切换指定列的排序状态：升序 -> 降序 -> 原始顺序。
    pub fn toggle_sort(&mut self, column: usize) -> Result<TableSortDirection, TableError> {
        if self.sort_strategy == TableSortStrategy::Preordered {
            return Err(TableError::SortingUnavailableForPreorderedRows);
        }
        if column >= self.columns.len() {
            return Err(TableError::ColumnOutOfBounds(column));
        }
        if !self.columns[column].sortable {
            return Err(TableError::ColumnNotSortable(column));
        }
        let direction = match self.sort {
            Some(TableSort {
                column: current,
                direction,
            }) if current == column => match direction {
                TableSortDirection::Unsorted => TableSortDirection::Ascending,
                TableSortDirection::Ascending => TableSortDirection::Descending,
                TableSortDirection::Descending => TableSortDirection::Unsorted,
            },
            _ => TableSortDirection::Ascending,
        };
        self.sort = (direction != TableSortDirection::Unsorted).then_some(TableSort { column, direction });
        Ok(direction)
    }

    pub fn sort_by(&mut self, column: usize, direction: TableSortDirection) -> Result<(), TableError> {
        if self.sort_strategy == TableSortStrategy::Preordered {
            return Err(TableError::SortingUnavailableForPreorderedRows);
        }
        if column >= self.columns.len() {
            return Err(TableError::ColumnOutOfBounds(column));
        }
        if !self.columns[column].sortable {
            return Err(TableError::ColumnNotSortable(column));
        }
        self.sort = (direction != TableSortDirection::Unsorted).then_some(TableSort { column, direction });
        Ok(())
    }

    pub fn select_source_row(&mut self, row: Option<usize>) -> Result<(), TableError> {
        if let Some(row) = row {
            if row >= self.rows.len() {
                return Err(TableError::RowOutOfBounds(row));
            }
        }
        self.selected_source_row = row;
        Ok(())
    }

    pub fn set_checked(&mut self, row: usize, checked: bool) -> Result<(), TableError> {
        let target = self.rows.get_mut(row).ok_or(TableError::RowOutOfBounds(row))?;
        target.checked = checked;
        Ok(())
    }

    /// 生成当前过滤和排序后的行快照。
    pub fn visible_rows(&self) -> Vec<VisibleTableRow> {
        self.filtered_indices()
            .into_iter()
            .map(|source_index| VisibleTableRow {
                source_index,
                cells: self.rows[source_index].cells.clone(),
                checked: self.rows[source_index].checked,
                selected: self.selected_source_row == Some(source_index),
            })
            .collect()
    }

    fn validate_rows(expected: usize, rows: &[TableRow]) -> Result<(), TableError> {
        if let Some(row) = rows.iter().find(|row| row.cells.len() != expected) {
            return Err(TableError::RowWidthMismatch {
                expected,
                actual: row.cells.len(),
            });
        }
        Ok(())
    }

    fn matches_filters(&self, row: &TableRow) -> bool {
        self.filters.iter().all(|filter| {
            let matched = match filter.column {
                Some(column) => matches_value(&row.cells[column], filter),
                None => row.cells.iter().any(|cell| matches_value(cell, filter)),
            };
            match filter.selection {
                FilterSelection::Include => matched,
                FilterSelection::Exclude => !matched,
            }
        })
    }
}
