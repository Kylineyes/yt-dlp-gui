use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::tasks::{self, TaskSortColumn, TaskSortDirection};
use super::{AppWindow, TableRow as SlintTableRow};
use crate::app::contracts::Route;
use crate::design_system::i18n::Locale;
use crate::storage::{DownloadTask, DownloadTaskFilter, Storage};

struct TasksState {
    tasks: Vec<DownloadTask>,
    checked: Vec<bool>,
    sort_column: Option<TaskSortColumn>,
    sort_direction: TaskSortDirection,
    timers: Vec<slint::Timer>,
}

pub(super) fn install(ui: &AppWindow, storage: &'static Storage, locale: Rc<Cell<Locale>>) -> Rc<dyn Fn()> {
    let state = Rc::new(RefCell::new(TasksState {
        tasks: Vec::new(),
        checked: Vec::new(),
        sort_column: None,
        sort_direction: TaskSortDirection::Reset,
        timers: Vec::new(),
    }));

    let refresh: Rc<dyn Fn()> = {
        let state = Rc::clone(&state);
        let ui_weak = ui.as_weak();
        let locale = Rc::clone(&locale);
        Rc::new(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            reload(&ui, storage, &state, locale.get());
        })
    };

    let poll_state = Rc::clone(&state);
    let poll_ui = ui.as_weak();
    let poll_locale = Rc::clone(&locale);
    let poll_timer = slint::Timer::default();
    poll_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = poll_ui.upgrade() else { return };
        if ui.get_current_route() != Route::Tasks.index() {
            return;
        }
        if !tasks::has_active_tasks(&poll_state.borrow().tasks) {
            return;
        }
        reload(&ui, storage, &poll_state, poll_locale.get());
    });
    state.borrow_mut().timers.push(poll_timer);

    let sort_state = Rc::clone(&state);
    let sort_ui = ui.as_weak();
    let sort_locale = Rc::clone(&locale);
    ui.on_tasks_sort_requested(move |column, direction| {
        let Some(column) = TaskSortColumn::from_index(column) else {
            return;
        };
        let Some(direction) = TaskSortDirection::from_index(direction) else {
            return;
        };
        let Some(ui) = sort_ui.upgrade() else { return };
        {
            let mut state = sort_state.borrow_mut();
            state.sort_column = (direction != TaskSortDirection::Reset).then_some(column);
            state.sort_direction = direction;
            render(&ui, &state, sort_locale.get());
        }
    });

    let check_state = Rc::clone(&state);
    let check_ui = ui.as_weak();
    let check_locale = Rc::clone(&locale);
    ui.on_tasks_check_all_toggled(move |checked| {
        let Some(ui) = check_ui.upgrade() else { return };
        let mut state = check_state.borrow_mut();
        state.checked.fill(checked);
        render(&ui, &state, check_locale.get());
    });

    let check_state = Rc::clone(&state);
    let check_ui = ui.as_weak();
    ui.on_tasks_check_toggled(move |source_row, checked| {
        let Ok(source_row) = usize::try_from(source_row) else {
            return;
        };
        let Some(ui) = check_ui.upgrade() else { return };
        let mut state = check_state.borrow_mut();
        let Some(value) = state.checked.get_mut(source_row) else {
            return;
        };
        *value = checked;
        render(&ui, &state, locale.get());
    });

    refresh();
    refresh
}

fn reload(ui: &AppWindow, storage: &'static Storage, state: &Rc<RefCell<TasksState>>, locale: Locale) {
    let tasks = match storage.list_download_tasks(DownloadTaskFilter::default()) {
        Ok(tasks) => tasks,
        Err(error) => {
            eprintln!("读取下载任务历史失败：{error}");
            return;
        }
    };

    let mut state = state.borrow_mut();
    let checked_ids = state
        .tasks
        .iter()
        .zip(&state.checked)
        .filter_map(|(task, &checked)| checked.then_some(task.id))
        .collect::<Vec<_>>();
    state.tasks = tasks;
    state.checked = state.tasks.iter().map(|task| checked_ids.contains(&task.id)).collect();
    render(ui, &state, locale);
}

fn render(ui: &AppWindow, state: &TasksState, locale: Locale) {
    let order = tasks::sorted_task_indices(&state.tasks, state.sort_column, state.sort_direction);
    let mut progress_values = Vec::with_capacity(order.len());
    let rows = order
        .into_iter()
        .filter_map(|source_index| {
            let task = state.tasks.get(source_index)?;
            progress_values.push(i32::from(task.progress_percent.unwrap_or(0)));
            let row = tasks::task_row(task, locale);
            let cells = row.cells.into_iter().map(SharedString::from).collect::<Vec<_>>();
            Some(SlintTableRow {
                source_index: source_index as i32,
                cells: ModelRc::new(VecModel::from(cells)),
                checked: state.checked.get(source_index).copied().unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();

    ui.set_tasks_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_tasks_progress_values(ModelRc::new(VecModel::from(progress_values)));
    ui.set_tasks_sort_column(state.sort_column.map_or(-1, TaskSortColumn::index));
    ui.set_tasks_sort_direction(state.sort_direction.index());
}
