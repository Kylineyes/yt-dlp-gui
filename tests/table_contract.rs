use std::fs;

use yt_dlp_gui::table::{
    FilterMatch, FilterSelection, TableColumn, TableError, TableFilter, TableModel, TableRow, TableSortDirection,
    TableSortStrategy,
};

fn model() -> TableModel {
    TableModel::new(
        vec![
            TableColumn::new("名称", true, 2),
            TableColumn::new("类型", false, 1),
            TableColumn::new("状态", true, 1),
        ],
        vec![
            TableRow::new(vec!["Beta".into(), "视频".into(), "完成".into()]),
            TableRow::new(vec!["alpha".into(), "音频".into(), "等待".into()]),
            TableRow::new(vec!["Gamma".into(), "视频".into(), "完成".into()]),
        ],
    )
    .unwrap()
}

#[test]
fn table_accepts_arbitrary_columns_and_rows_without_a_ui_specific_shape() {
    let mut columns = Vec::new();
    let mut cells = Vec::new();
    for index in 0..101 {
        columns.push(TableColumn::new(format!("列 {index}"), index % 2 == 0, 1));
        cells.push(format!("值 {index}"));
    }

    let table = TableModel::new(columns, vec![TableRow::new(cells)]).unwrap();
    assert_eq!(table.columns().len(), 101);
    assert_eq!(table.visible_rows()[0].cells.len(), 101);
}

#[test]
fn columns_can_override_the_theme_font_for_monospace_values() {
    let column = TableColumn::new("格式 ID", true, 1).font_family("Cascadia Mono");
    assert_eq!(column.font_family, "Cascadia Mono");

    let default_column = TableColumn::new("名称", false, 1);
    assert!(default_column.font_family.is_empty());
}

#[test]
fn visible_batch_checking_only_changes_filtered_rows() {
    let mut table = model();
    table.set_checked(1, true).unwrap();
    table
        .set_filters(vec![TableFilter::new(
            Some(1),
            "视频",
            FilterMatch::Equals,
            FilterSelection::Include,
        )])
        .unwrap();

    assert_eq!(table.visible_source_indices(), vec![0, 2]);
    assert!(!table.all_visible_checked());
    table.set_all_visible_checked(true);
    assert!(table.rows()[0].checked);
    assert!(table.rows()[2].checked);
    assert!(table.rows()[1].checked);
    assert!(table.all_visible_checked());

    table.set_all_visible_checked(false);
    assert!(!table.rows()[0].checked);
    assert!(!table.rows()[2].checked);
    assert!(table.rows()[1].checked);
}

#[test]
fn visible_batch_checking_works_for_preordered_rows_and_empty_results() {
    let mut table = model();
    table
        .replace_rows_preordered(vec![
            TableRow::new(vec!["Gamma".into(), "视频".into(), "完成".into()]),
            TableRow::new(vec!["alpha".into(), "音频".into(), "等待".into()]),
        ])
        .unwrap();
    table.set_all_visible_checked(true);
    assert!(table.rows().iter().all(|row| row.checked));

    table
        .set_filters(vec![TableFilter::new(
            Some(0),
            "missing",
            FilterMatch::Equals,
            FilterSelection::Include,
        )])
        .unwrap();
    assert!(table.visible_source_indices().is_empty());
    assert!(!table.all_visible_checked());
    table.set_all_visible_checked(false);
    assert!(table.rows().iter().all(|row| row.checked));
}

#[test]
fn rows_can_be_inserted_at_any_valid_position_and_keep_selection_identity() {
    let mut table = model();
    table.select_source_row(Some(2)).unwrap();

    table
        .insert_row(1, TableRow::new(vec!["Inserted".into(), "视频".into(), "等待".into()]))
        .unwrap();

    assert_eq!(table.selected_source_row(), Some(3));
    assert_eq!(table.rows()[1].cells[0], "Inserted");
    assert_eq!(table.visible_rows()[3].source_index, 3);
}

#[test]
fn replacing_rows_clears_selection_and_rejects_malformed_replacements_atomically() {
    let mut table = model();
    table.select_source_row(Some(1)).unwrap();

    table
        .set_rows(vec![
            TableRow::new(vec!["New A".into(), "视频".into(), "完成".into()]),
            TableRow::new(vec!["New B".into(), "音频".into(), "等待".into()]),
        ])
        .unwrap();
    assert_eq!(table.selected_source_row(), None);
    assert_eq!(table.rows()[0].cells[0], "New A");

    let result = table.replace_rows(vec![TableRow::new(vec!["bad".into()])]);
    assert_eq!(result, Err(TableError::RowWidthMismatch { expected: 3, actual: 1 }));
    assert_eq!(table.rows().len(), 2);
    assert_eq!(table.rows()[0].cells[0], "New A");
}

#[test]
fn preordered_replacements_preserve_consumer_order_without_secondary_sorting() {
    let mut table = model();
    table
        .replace_rows_preordered(vec![
            TableRow::new(vec!["Gamma".into(), "视频".into(), "完成".into()]),
            TableRow::new(vec!["alpha".into(), "音频".into(), "等待".into()]),
            TableRow::new(vec!["Beta".into(), "视频".into(), "完成".into()]),
        ])
        .unwrap();

    assert_eq!(table.sort_strategy(), TableSortStrategy::Preordered);
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        table.toggle_sort(0),
        Err(TableError::SortingUnavailableForPreorderedRows)
    );
}

#[test]
fn right_and_middle_clicks_do_not_request_row_selection() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ui/components/generic-table.slint"
    ))
    .unwrap();
    let dispatch = source.split("function dispatch-pointer").nth(1).unwrap();

    let left_click = dispatch.find("if (button == PointerEventButton.left) {").unwrap();
    let selection = dispatch.find("selection-requested(row.source-index);").unwrap();
    assert!(left_click < selection);
    assert!(dispatch.contains("right-clicked(row.source-index);"));
    assert!(dispatch.contains("middle-clicked(row.source-index);"));
}

#[test]
fn sortable_columns_use_case_insensitive_dictionary_order_and_skip_non_sortable_columns() {
    let mut table = model();
    assert_eq!(table.toggle_sort(0).unwrap(), TableSortDirection::Ascending);
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![1, 0, 2]
    );
    assert_eq!(table.toggle_sort(0).unwrap(), TableSortDirection::Descending);
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![2, 0, 1]
    );
    assert_eq!(table.toggle_sort(0).unwrap(), TableSortDirection::Unsorted);
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(table.toggle_sort(1), Err(TableError::ColumnNotSortable(1)));
}

#[test]
fn include_and_exclude_filters_operate_before_sorting_and_preserve_source_indices() {
    let mut table = model();
    table
        .set_filters(vec![TableFilter::new(
            Some(1),
            "视频",
            FilterMatch::Equals,
            FilterSelection::Include,
        )])
        .unwrap();
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );

    table
        .set_filters(vec![TableFilter::new(
            Some(2),
            "完成",
            FilterMatch::Equals,
            FilterSelection::Exclude,
        )])
        .unwrap();
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn filters_can_match_any_cell_and_support_partial_text() {
    let mut table = model();
    table
        .set_filters(vec![TableFilter::new(
            None,
            "ALP",
            FilterMatch::Contains,
            FilterSelection::Include,
        )])
        .unwrap();
    assert_eq!(
        table
            .visible_rows()
            .iter()
            .map(|row| row.source_index)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn checked_state_belongs_to_the_source_row_and_survives_filtering() {
    let mut table = model();
    table.set_checked(2, true).unwrap();
    table
        .set_filters(vec![TableFilter::new(
            Some(1),
            "视频",
            FilterMatch::Equals,
            FilterSelection::Include,
        )])
        .unwrap();

    let rows = table.visible_rows();
    assert_eq!(rows[1].source_index, 2);
    assert!(rows[1].checked);
}

#[test]
fn generic_table_exposes_extended_behavior_without_changing_row_structures() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ui/components/generic-table.slint"
    ))
    .unwrap();

    assert!(!source.contains("text: \"✓\""));
    assert!(source.contains("import { I18n } from \"../design/i18n.slint\";"));
    assert!(source.contains("menu-reset-widths-label: I18n.table-reset-widths"));
    assert!(source.contains("menu-reset-titles-label: I18n.table-reset-titles"));
    assert!(source.contains("menu-show-columns-label: I18n.table-show-columns"));
    assert!(source.contains("component TableCheckMark inherits Path"));
    assert!(source.contains("check-all-toggled(checked: bool)"));
    assert!(source.contains("resizable-columns: false"));
    assert!(source.contains("column-widths: []"));
    assert!(source.contains("column-visibility: []"));
    assert!(source.contains("ContextMenuArea"));
    assert!(source.contains("MenuSeparator"));
    assert!(source.contains("progress-column: -1"));
    assert!(source.contains("progress-values: []"));
    assert!(source.contains("progress-color: Theme.accent"));
    assert!(source.contains("progress-values.length == root.row-count"));
    assert!(source.contains("min(max(root.progress-values[root.row-index], 0), 100)"));
    assert!(source.contains("table-max-height: 0px"));
    assert!(source.contains("ScrollBarPolicy.as-needed"));
    assert!(source.contains("visibility-reset-epoch"));
    assert!(source.contains("width-reset-epoch"));

    let column_struct = source.split("export struct TableColumn {").nth(1).unwrap();
    let row_struct = source.split("export struct TableRow {").nth(1).unwrap();
    assert_eq!(column_struct.split('}').next().unwrap().matches(',').count(), 5);
    assert_eq!(row_struct.split('}').next().unwrap().matches(',').count(), 3);
}

#[test]
fn header_context_menu_is_topmost_without_handling_left_click_business_events() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ui/components/generic-table.slint"
    ))
    .unwrap();
    let header = source
        .split("header := Rectangle {")
        .nth(1)
        .unwrap()
        .split("if root.table-max-height > 0px : ScrollView {")
        .next()
        .unwrap();

    let columns = header
        .find("for column[index] in root.columns : TableHeaderColumn {")
        .unwrap();
    let menu = header
        .find("if root.column-hiding-enabled : ContextMenuArea {")
        .unwrap();
    assert!(columns < menu);

    let menu_block = &header[menu..];
    assert!(menu_block.contains("width: parent.width;"));
    assert!(menu_block.contains("height: parent.height;"));
    assert!(menu_block.contains("MenuSeparator {"));
    assert!(menu_block.contains("for column[index] in root.columns : MenuItem {"));
    assert!(!menu_block.contains("clicked =>"));
    assert!(!menu_block.contains("pointer-event"));
    assert!(!menu_block.contains("selection-requested"));
    assert!(!menu_block.contains("sort-requested"));
    assert!(!menu_block.contains("column-resized"));

    assert!(source.contains("clicked => { root.sort-clicked(); }"));
    assert!(source.contains("event.kind == PointerEventKind.down && event.button == PointerEventButton.left"));
    assert!(source.contains("if (self.pressed) {"));
    assert!(source.contains("mouse-cursor: MouseCursor.col-resize;"));
}

#[test]
fn header_and_rows_share_content_geometry_and_visibility_rules() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ui/components/generic-table.slint"
    ))
    .unwrap();

    assert!(source.contains(
        "x: 0px;\n                width: parent.width;\n                height: parent.height;\n                spacing: Theme.spacing-compact;\n\n                if root.show-check-column : Rectangle"
    ));
    assert!(source.contains(
        "x: 0px;\n        width: parent.width;\n        height: parent.height;\n        spacing: Theme.spacing-compact;\n\n        if root.show-check-column : TableCheckBox"
    ));
    assert!(
        source
            .matches("root.column-visibility.length == 0 || root.column-visibility.length <= ")
            .count()
            >= 3
    );
    assert_eq!(
        source
            .matches("root.column-widths.length > column-index ? root.column-widths[column-index] : 0px")
            .count(),
        1
    );
    assert_eq!(
        source
            .matches("root.column-widths.length > column-index && root.column-widths[column-index] > 0px")
            .count(),
        3
    );
    assert_eq!(
        source
            .matches("max(root.columns[column-index].width-weight, 1.0)")
            .count(),
        1
    );
    assert_eq!(source.matches("max(root.width-weight, 1.0)").count(), 1);
}

#[test]
fn header_check_and_menu_resets_keep_single_state_update_paths() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ui/components/generic-table.slint"
    ))
    .unwrap();
    let header_check = source
        .split("if root.show-check-column : Rectangle {")
        .nth(1)
        .unwrap()
        .split("for column[index] in root.columns : TableHeaderColumn {")
        .next()
        .unwrap();

    assert!(header_check.contains("checked: root.header-all-checked;"));
    assert_eq!(
        header_check
            .matches("root.check-all-toggled(!root.header-all-checked);")
            .count(),
        1
    );
    assert!(source.contains("activated => { root.width-reset-epoch += 1; }"));
    assert!(source.contains("activated => { root.visibility-reset-epoch += 1; }"));
    assert!(source.contains("root.column-widths[root.column-index] = 0px;"));
    assert!(source.contains("root.column-visibility[root.column-index] = true;"));
    assert!(source.contains("root.column-visibility[index] = !root.column-visibility[index];"));
}

#[test]
fn malformed_rows_and_filters_are_rejected_without_panicking() {
    assert!(matches!(
        TableModel::new(vec![TableColumn::new("一", false, 1)], vec![TableRow::new(Vec::new())],),
        Err(TableError::RowWidthMismatch { expected: 1, actual: 0 })
    ));

    let mut table = model();
    assert_eq!(
        table.set_filters(vec![TableFilter::new(
            Some(99),
            "x",
            FilterMatch::Equals,
            FilterSelection::Include,
        )]),
        Err(TableError::ColumnOutOfBounds(99))
    );
    assert_eq!(
        table.insert_row(99, TableRow::new(vec!["a".into(), "b".into(), "c".into()])),
        Err(TableError::RowOutOfBounds(99))
    );
}
