# 通用表格模块使用说明

`table` 由 Rust 数据控制器和 Slint 视图组件组成。它只提供通用表格能力，不绑定检索页面、下载任务或其他业务对象；现有 Search 页面不会自动切换到该组件。

当前稳定契约包括：

- 行选择只由左键触发；右键和中键仅发送各自的点击回调；
- 所有行回调都携带过滤前的 `source_index`；
- 列可以覆盖主题默认字体，适合为格式 ID 和编码列使用等宽字体；
- `replace_rows` / `set_rows` 可以原子替换结果集合；
- `Preordered` 策略保证消费者提交的预排序结果不会被二次排序。

## 一、模块结构

```text
src/table/
├── mod.rs       # 对外导出稳定 API
├── column.rs    # 表格列描述
├── row.rs       # 原始行和显示行快照
├── filter.rs    # 过滤条件与匹配规则
├── sort.rs      # 排序方向、排序策略和字典序比较
├── error.rs     # 表格操作错误
├── model.rs     # TableModel 数据控制器
└── README.md    # 本使用说明

ui/components/generic-table.slint  # Slint 表格视图和交互回调
```

`mod.rs` 只负责模块拆分和公开导出，业务侧仍然从 `yt_dlp_gui::table` 统一导入类型，不需要依赖内部文件路径。

## 二、核心数据关系

```text
原始 rows
   │
   ├── filters：过滤，不改变原始行号
   ├── Dictionary：按当前列执行不区分大小写的字典序
   ├── Preordered：保留消费者提交的顺序，不执行二次排序
   └── visible_rows()：生成显示快照
                         │
                         └── source_index -> 原始 rows 的索引
```

`TableRow::checked` 和 `TableModel::selected_source_row()` 都属于过滤前的原始行状态。表格过滤或排序后，显示行通过 `VisibleTableRow::source_index` 映射回原始行，因此勾选和点击回调不会错配到其他行。

所有行必须拥有与列数量相同的单元格数量；不满足时构造、插入或结果集合替换操作返回 `TableError::RowWidthMismatch`。

## 三、Rust 数据控制器

### 3.1 创建列和行

```rust
use yt_dlp_gui::table::{TableColumn, TableModel, TableRow};

let columns = vec![
    TableColumn::new("名称", true, 2),
    TableColumn::new("类型", false, 1),
    TableColumn::new("状态", true, 1),
];

let rows = vec![
    TableRow::new(vec!["Beta".into(), "视频".into(), "完成".into()]),
    TableRow::new(vec!["alpha".into(), "音频".into(), "等待".into()]),
];

let mut table = TableModel::new(columns, rows)?;
```

`TableColumn::new` 的参数依次是：

| 参数 | 含义 |
| --- | --- |
| `title` | 标题栏显示文本，业务侧负责提供已解析的 i18n 文案 |
| `sortable` | 是否允许点击标题触发排序 |
| `width_weight` | 列的相对宽度权重，`0` 会自动按 `1` 处理 |

`font_family` 不是构造函数参数，而是可选的列属性；为空时使用主题 UI 字体。

需要让格式 ID、视频编码或音频编码使用等宽字体时，可以在列级配置字体：

```rust
fn format_id_column(mono_font_family: &str) -> TableColumn {
    TableColumn::new("格式 ID", true, 1).font_family(mono_font_family)
}
```

实际页面接入时，应把 `Theme.mono-font-family` 对应的主题字体值传给领域列定义，不要在组件中另外硬编码字体名称。

表格只接收字符串单元格。业务对象应在交给 `TableModel` 前转换为展示文本，这样表格不会耦合业务结构。

### 3.2 插入行

插入位置使用过滤前的原始行索引：

```rust
let row = TableRow::new(vec!["Inserted".into(), "视频".into(), "等待".into()]);
table.insert_row(1, row)?;

// 追加到原始数据末尾
table.push_row(TableRow::new(vec![
    "Last".into(),
    "音频".into(),
    "完成".into(),
]))?;
```

如果已有选中行位于插入点之后，控制器会自动调整 `selected_source_row`，继续指向原来的逻辑行。

### 3.3 替换结果集合

使用 `replace_rows` 或 `set_rows` 替换整个原始结果集合。替换会清除选中行，避免旧结果的行号误指向新结果；过滤条件和当前排序策略保持不变。

```rust
table.replace_rows(new_rows)?;
// 或使用语义等价的别名
table.set_rows(new_rows)?;
```

替换前会先校验所有行的单元格数量。校验失败时不会修改已有结果集合。

如果结果已经由业务层按照数值、时间或其他自定义规则排好顺序，应使用预排序接口：

```rust
table.replace_rows_preordered(new_rows)?;
```

`replace_rows_preordered` 会切换到 `TableSortStrategy::Preordered`。之后 `visible_rows()` 只执行过滤和快照映射，不会二次排序；调用 `toggle_sort` 或 `sort_by` 会返回 `TableError::SortingUnavailableForPreorderedRows`。

需要恢复通用字典序策略时：

```rust
use yt_dlp_gui::table::TableSortStrategy;

table.set_sort_strategy(TableSortStrategy::Dictionary);
```

### 3.4 选择和勾选

```rust
table.select_source_row(Some(0))?;
table.set_checked(0, true)?;

assert_eq!(table.selected_source_row(), Some(0));
assert!(table.rows()[0].checked);
```

清除选择使用 `table.select_source_row(None)?`。选择和勾选都使用原始行索引，不使用过滤或排序后的显示位置。

### 3.5 排序

排序只作用于 `visible_rows()` 的输出顺序，不重排 `TableModel::rows()`：

```rust
use yt_dlp_gui::table::TableSortDirection;

// 点击一次：升序
assert_eq!(table.toggle_sort(0)?, TableSortDirection::Ascending);

// 点击第二次：降序
assert_eq!(table.toggle_sort(0)?, TableSortDirection::Descending);

// 点击第三次：恢复原始顺序
assert_eq!(table.toggle_sort(0)?, TableSortDirection::Unsorted);
```

也可以直接指定状态：

```rust
table.sort_by(0, TableSortDirection::Ascending)?;
```

不可排序列会返回 `TableError::ColumnNotSortable`。默认比较规则是不区分大小写的字典序；需要数值、时间或业务优先级排序时，应在业务层完成转换或排序策略，不要让通用表格猜测字段含义。

### 3.6 过滤

过滤条件可以限定列，也可以匹配整行任意单元格：

```rust
use yt_dlp_gui::table::{FilterMatch, FilterSelection, TableFilter};

let filters = vec![
    // 只保留“类型”等于“视频”的行
    TableFilter::new(
        Some(1),
        "视频",
        FilterMatch::Equals,
        FilterSelection::Include,
    ),
    // 排除名称中包含“测试”的行
    TableFilter::new(
        Some(0),
        "测试",
        FilterMatch::Contains,
        FilterSelection::Exclude,
    ),
];

table.set_filters(filters)?;
```

可用匹配方式：

- `Equals`：完全相等；
- `Contains`：包含文本；
- `StartsWith`：以文本开头；
- `EndsWith`：以文本结尾。

`TableFilter::new` 默认不区分大小写。需要区分大小写时：

```rust
let filter = TableFilter::new(
    None,
    "MP4",
    FilterMatch::Equals,
    FilterSelection::Include,
)
.case_sensitive(true);
```

多个过滤条件按 AND 关系同时生效。清空过滤条件：

```rust
table.clear_filters();
```

### 3.7 获取显示快照

```rust
let visible_rows = table.visible_rows();

for row in visible_rows {
    println!(
        "显示行对应原始第 {} 行，已勾选：{}，单元格：{:?}",
        row.source_index,
        row.checked,
        row.cells,
    );
}
```

不要使用 `visible_rows()` 返回值中的位置作为业务索引；应使用 `source_index`。

## 四、Slint 视图组件

组件文件为 [`ui/components/generic-table.slint`](../../ui/components/generic-table.slint)。可以在其他 Slint 页面中直接导入：

```slint
import { GenericTable, TableColumn, TableRow, TableSortDirection } from "../components/generic-table.slint";

export component ExampleTable inherits Rectangle {
    in property <[TableColumn]> table-columns: [];
    in property <[TableRow]> table-rows: [];
    in property <int> selected-source-row: -1;

    callback sort-requested(int, TableSortDirection);
    callback selection-requested(int);
    callback row-left-clicked(int);
    callback row-right-clicked(int);
    callback row-middle-clicked(int);
    callback row-check-toggled(int, bool);

    GenericTable {
        columns: root.table-columns;
        rows: root.table-rows;
        rows-selectable: true;
        show-check-column: true;
        selected-source-row: root.selected-source-row;

        sort-requested(column, direction) => {
            root.sort-requested(column, direction);
        }
        selection-requested(source-row) => {
            root.selection-requested(source-row);
        }
        left-clicked(source-row) => { root.row-left-clicked(source-row); }
        right-clicked(source-row) => { root.row-right-clicked(source-row); }
        middle-clicked(source-row) => { root.row-middle-clicked(source-row); }
        check-toggled(source-row, checked) => {
            root.row-check-toggled(source-row, checked);
        }
    }
}
```

组件属性说明：

| 属性 | 默认值 | 作用 |
| --- | --- | --- |
| `columns` | `[]` | 标题列描述 |
| `rows` | `[]` | 当前应显示的行快照 |
| `TableColumn.font-family` | `""` | 列字体；为空时跟随主题 UI 字体 |
| `rows-selectable` | `false` | 是否响应行选择和鼠标按钮回调 |
| `show-check-column` | `false` | 是否显示最前方勾选列 |
| `selected-source-row` | `-1` | 当前选中的原始行号 |
| `intrinsic-height` | 自动计算 | 表头加所有显示行的真实高度 |

鼠标回调语义：

| 回调 | 触发条件 | 是否请求选择 |
| --- | --- | --- |
| `selection-requested` | `rows-selectable` 为 `true` 且左键释放 | 是 |
| `left-clicked` | `rows-selectable` 为 `true` 且左键释放 | 与 `selection-requested` 同次触发 |
| `right-clicked` | `rows-selectable` 为 `true` 且右键释放 | 否 |
| `middle-clicked` | `rows-selectable` 为 `true` 且中键释放 | 否 |
| `check-toggled` | 勾选列可见并点击勾选框 | 不受行选择开关影响 |

右键和中键不会隐式改变 `selected-source-row`。使用方如果希望右键菜单主动选择行，应在自己的 `right-clicked` 回调中显式处理。所有这些回调的行参数均为过滤前的 `source-index`。

组件不会自行改变业务数据顺序。标题点击后通过 `sort-requested` 通知使用方，使用方应调用 `TableModel::toggle_sort` 或 `sort_by`，再把新的 `visible_rows()` 快照传回 Slint。

## 五、Rust 与 Slint 数据转换

`AppWindow` 已导出表格所需的 Slint 数据结构：

```rust
use slint::{ModelRc, SharedString, VecModel};
use yt_dlp_gui::app::{
    TableColumn as UiTableColumn,
    TableRow as UiTableRow,
    TableSortDirection as UiTableSortDirection,
};
use yt_dlp_gui::table::{TableModel, TableSortDirection};

fn ui_columns(table: &TableModel) -> ModelRc<UiTableColumn> {
    ModelRc::new(VecModel::from(
        table
            .columns()
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let sort_direction = match table
                    .sort()
                    .filter(|sort| sort.column == index)
                    .map(|sort| sort.direction)
                {
                    Some(TableSortDirection::Ascending) => UiTableSortDirection::Ascending,
                    Some(TableSortDirection::Descending) => UiTableSortDirection::Descending,
                    _ => UiTableSortDirection::Unsorted,
                };
                UiTableColumn {
                    title: SharedString::from(column.title.as_str()),
                    sortable: column.sortable,
                    width_weight: column.width_weight as f32,
                    font_family: SharedString::from(column.font_family.as_str()),
                    sort_direction,
                }
            })
            .collect::<Vec<_>>(),
    ))
}

fn ui_rows(table: &TableModel) -> ModelRc<UiTableRow> {
    ModelRc::new(VecModel::from(
        table
            .visible_rows()
            .into_iter()
            .map(|row| UiTableRow {
                source_index: row.source_index as i32,
                cells: ModelRc::new(VecModel::from(
                    row.cells
                        .iter()
                        .map(|cell| SharedString::from(cell.as_str()))
                        .collect::<Vec<_>>(),
                )),
                checked: row.checked,
            })
            .collect::<Vec<_>>(),
    ))
}
```

实际页面中通常会把这些快照通过页面属性传入 `GenericTable`，并把 `table.selected_source_row().map_or(-1, |row| row as i32)` 传给 `selected-source-row`。回调中的 `source-row` 应直接交给对应业务逻辑，不要转换成当前显示位置。