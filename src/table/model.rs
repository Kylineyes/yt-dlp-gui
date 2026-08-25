use super::filter::matches_value;
use super::{
    compare_lexicographic, FilterSelection, TableColumn, TableError, TableFilter, TableRow, TableSort,
    TableSortDirection, VisibleTableRow,
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
    selected_source_row: Option<usize>,
}

impl TableModel {
    pub fn new(columns: Vec<TableColumn>, rows: Vec<TableRow>) -> Result<Self, TableError> {
        let expected = columns.len();
        if let Some(row) = rows.iter().find(|row| row.cells.len() != expected) {
            return Err(TableError::RowWidthMismatch {
                expected,
                actual: row.cells.len(),
            });
        }
        Ok(Self {
            columns,
            rows,
            filters: Vec::new(),
            sort: None,
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

    pub fn selected_source_row(&self) -> Option<usize> {
        self.selected_source_row
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
        let mut indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.matches_filters(row))
            .map(|(index, _)| index)
            .collect();

        if let Some(TableSort { column, direction }) = self.sort {
            indices.sort_by(|&left, &right| {
                let ordering = compare_lexicographic(&self.rows[left].cells[column], &self.rows[right].cells[column]);
                if direction == TableSortDirection::Descending {
                    ordering.reverse()
                } else {
                    ordering
                }
                .then_with(|| left.cmp(&right))
            });
        }

        indices
            .into_iter()
            .map(|source_index| VisibleTableRow {
                source_index,
                cells: self.rows[source_index].cells.clone(),
                checked: self.rows[source_index].checked,
                selected: self.selected_source_row == Some(source_index),
            })
            .collect()
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
