/// 过滤的匹配方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMatch {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
}

/// 过滤结果的方向。
///
/// Include 表示命中条件的行保留，Exclude 表示命中条件的行排除。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSelection {
    Include,
    Exclude,
}

/// 一个过滤条件可以限定某一列，也可以在整行任意单元格中匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableFilter {
    pub column: Option<usize>,
    pub value: String,
    pub match_kind: FilterMatch,
    pub selection: FilterSelection,
    pub case_sensitive: bool,
}

impl TableFilter {
    pub fn new(
        column: Option<usize>,
        value: impl Into<String>,
        match_kind: FilterMatch,
        selection: FilterSelection,
    ) -> Self {
        Self {
            column,
            value: value.into(),
            match_kind,
            selection,
            case_sensitive: false,
        }
    }

    pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }
}

/// 判断一个单元格是否命中过滤条件。
///
/// 该函数只在表格控制器内部使用，保证列过滤和整行任意单元格过滤采用同一套规则。
pub(crate) fn matches_value(value: &str, filter: &TableFilter) -> bool {
    let (value, pattern) = if filter.case_sensitive {
        (value.to_owned(), filter.value.clone())
    } else {
        (value.to_lowercase(), filter.value.to_lowercase())
    };
    match filter.match_kind {
        FilterMatch::Equals => value == pattern,
        FilterMatch::Contains => value.contains(&pattern),
        FilterMatch::StartsWith => value.starts_with(&pattern),
        FilterMatch::EndsWith => value.ends_with(&pattern),
    }
}
