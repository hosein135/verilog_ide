//! Main IDE application (iced, VS Code–inspired layout).

use crate::editor::{
    self, cursor_from_line_col, cursor_line_col, EDITOR_FONT_SIZE, EDITOR_LINE_HEIGHT,
    EDITOR_PADDING,
};
use crate::verilog_highlighter::{
    self, Settings as VerilogHighlightSettings, VerilogHighlighter,
};
use crate::project::{
    collect_dir_paths, initial_editor_files, is_wave_path, load_file, locate_half_adder_sample,
    save_file, FileView, IdeProject, OpenFile, TreeNode,
};
use crate::sim::{self, SimResult};
use crate::templates;
use crate::wave_tools::{self, WaveToolKind, WaveToolOutput};
use crate::waveform::{self, ViewMode, WaveNav, WaveView};
use iced::keyboard::{self, Key};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, mouse_area, row, scrollable,
    stack, text, text_editor, text_input, Column, Space,
};
use iced::{
    Alignment, Border, Color, Element, Fill, Font, Length, Padding, Point, Shadow, Subscription,
    Task, Theme,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const ACTIVITY_WIDTH: f32 = 56.0;
const SIDEBAR_WIDTH: f32 = 280.0;
const BOTTOM_HEIGHT: f32 = 168.0;
const MENU_HEIGHT: f32 = 42.0;
const TAB_HEIGHT: f32 = 38.0;
const STATUS_HEIGHT: f32 = 26.0;

// Ink, teal, and amber. The editor sits on the darkest surface.
const BG_EDITOR: Color = Color::from_rgb(0.07, 0.09, 0.12);
const BG_SIDEBAR: Color = Color::from_rgb(0.09, 0.12, 0.16);
const BG_ACTIVITY: Color = Color::from_rgb(0.05, 0.07, 0.10);
const BG_TABS: Color = Color::from_rgb(0.08, 0.11, 0.15);
const BG_TAB_ACTIVE: Color = Color::from_rgb(0.11, 0.16, 0.21);
const BG_STATUS: Color = Color::from_rgb(0.08, 0.32, 0.36);
const BG_MENU: Color = Color::from_rgb(0.06, 0.08, 0.11);
const FG_MUTED: Color = Color::from_rgb(0.55, 0.63, 0.70);
const FG_TEXT: Color = Color::from_rgb(0.90, 0.93, 0.96);
const BORDER: Color = Color::from_rgb(0.18, 0.24, 0.31);
const ACCENT: Color = Color::from_rgb(0.31, 0.82, 0.84);
const AMBER: Color = Color::from_rgb(0.93, 0.72, 0.36);
const RUN_GREEN: Color = Color::from_rgb(0.16, 0.58, 0.42);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BottomTab {
    Console,
    Problems,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TopMenu {
    File,
    Edit,
    View,
    Run,
    Help,
}

#[derive(Clone)]
enum Dialog {
    NewFile { name: String },
    NewFolder { name: String },
    Find { query: String },
    About,
}

pub struct VerilogIde {
    project: Option<IdeProject>,
    tree: Option<TreeNode>,
    expanded: HashSet<PathBuf>,
    open: Vec<OpenFile>,
    active: Option<usize>,
    editor_content: text_editor::Content,
    editor_scroll_y: f32,
    editor_view_h: f32,
    console: String,
    problems: Vec<String>,
    bottom: BottomTab,
    bottom_visible: bool,
    status: String,
    dialog: Option<Dialog>,
    menu_open: Option<TopMenu>,
    search_query: String,
    sim_running: bool,
    tool_running: bool,
    view_mode: ViewMode,
    waveforms: HashMap<PathBuf, WaveStatus>,
}

enum WaveStatus {
    Loading,
    Ready(WaveView),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenProject,
    ProjectPicked(Result<Option<PathBuf>, DialogError>),
    OpenFilePicker,
    FilePicked(Result<Option<PathBuf>, DialogError>),
    CreateSample,
    OpenFile(PathBuf),
    ToggleDir(PathBuf),
    SelectTab(usize),
    CloseTab(usize),
    EditorAction(text_editor::Action),
    EditorScrolled { offset_y: f32, view_h: f32 },
    EditorPage(i32),
    Save,
    SaveAll,
    CloseProject,
    RefreshExplorer,
    ShowNewFile,
    ShowNewFolder,
    ShowFind,
    ShowAbout,
    DialogInput(String),
    DialogConfirm,
    DialogCancel,
    BottomTabSelected(BottomTab),
    ToggleBottomPanel,
    ClearBottom,
    MenuToggle(TopMenu),
    MenuClose,
    RunSim,
    SimFinished(SimResult),
    AnonymizeVcd,
    VcdToSaif,
    WaveToolFinished(Result<WaveToolOutput, String>),
    SetViewMode(ViewMode),
    WaveNav(WaveNav),
    WaveLoaded(PathBuf, Result<WaveView, String>),
}

#[derive(Debug, Clone)]
pub enum DialogError {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Cancelled,
}

pub fn run() -> iced::Result {
    iced::application("Verilog IDE", VerilogIde::update, VerilogIde::view)
        .theme(|_| Theme::Dark)
        .subscription(VerilogIde::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1360.0, 840.0),
            min_size: Some(iced::Size::new(960.0, 600.0)),
            ..Default::default()
        })
        .run_with(VerilogIde::new)
}

impl VerilogIde {
    fn new() -> (Self, Task<Message>) {
        let mut ide = Self {
            project: None,
            tree: None,
            expanded: HashSet::new(),
            open: Vec::new(),
            active: None,
            editor_content: text_editor::Content::new(),
            editor_scroll_y: 0.0,
            editor_view_h: 480.0,
            console: "Verilog IDE ready.\n".into(),
            problems: Vec::new(),
            bottom: BottomTab::Console,
            bottom_visible: true,
            status: "Ready".into(),
            dialog: None,
            menu_open: None,
            search_query: String::new(),
            sim_running: false,
            tool_running: false,
            view_mode: ViewMode::Waveform,
            waveforms: HashMap::new(),
        };
        let task = if let Some(sample) = locate_half_adder_sample() {
            ide.open_project(sample);
            ide.log("Half adder is open. Press F5 to simulate with xezim.\n");
            editor::scroll_to_y(0.0)
        } else {
            Task::none()
        };
        (ide, task)
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::on_key_press(|key, _modifiers| {
            if matches!(key, Key::Named(keyboard::key::Named::F5)) {
                Some(Message::RunSim)
            } else {
                None
            }
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MenuToggle(menu) => {
                self.menu_open = if self.menu_open == Some(menu) {
                    None
                } else {
                    Some(menu)
                };
                Task::none()
            }
            Message::MenuClose => {
                self.menu_open = None;
                Task::none()
            }
            Message::OpenProject => {
                self.menu_open = None;
                Task::perform(pick_folder("Open Folder"), Message::ProjectPicked)
            }
            Message::ProjectPicked(Ok(Some(path))) => {
                self.open_folder_path(path);
                editor::scroll_to_y(0.0)
            }
            Message::ProjectPicked(_) => Task::none(),
            Message::OpenFilePicker => {
                self.menu_open = None;
                Task::perform(pick_file("Open File"), Message::FilePicked)
            }
            Message::FilePicked(Ok(Some(path))) => {
                let load = if path.is_dir() {
                    self.open_folder_path(path);
                    Task::none()
                } else {
                    if self.project.is_none() {
                        if let Some(parent) = path.parent() {
                            self.open_project(parent.to_path_buf());
                        }
                    }
                    self.open_path(&path)
                };
                Task::batch([load, editor::scroll_to_y(0.0)])
            }
            Message::FilePicked(_) => Task::none(),
            Message::CreateSample => {
                self.menu_open = None;
                if let Some(sample) = locate_half_adder_sample() {
                    self.open_project(sample);
                    self.log(
                        "Opened the half-adder sample (half_adder.v + half_adder_tb.v).\nPress F5 to simulate with xezim and write half_adder.vcd.\n",
                    );
                    editor::scroll_to_y(0.0)
                } else {
                    self.log_err("Bundled half-adder sample was not found (samples/half_adder).");
                    Task::none()
                }
            }
            Message::CloseProject => {
                self.menu_open = None;
                self.sync_editor_to_active();
                self.project = None;
                self.tree = None;
                self.expanded.clear();
                self.open.clear();
                self.active = None;
                self.waveforms.clear();
                self.status = "Closed folder".into();
                self.reload_editor()
            }
            Message::RefreshExplorer => {
                if let Some(project) = self.project.as_ref() {
                    self.tree = Some(project.refresh_tree());
                }
                Task::none()
            }
            Message::OpenFile(path) => {
                let load = self.open_path(&path);
                Task::batch([load, editor::scroll_to_y(0.0)])
            }
            Message::ToggleDir(path) => {
                if self.expanded.contains(&path) {
                    self.expanded.remove(&path);
                } else {
                    self.expanded.insert(path);
                }
                Task::none()
            }
            Message::SelectTab(idx) => {
                self.sync_editor_to_active();
                self.active = Some(idx);
                self.reload_editor()
            }
            Message::CloseTab(idx) => {
                self.sync_editor_to_active();
                if self.open.get(idx).map(|f| f.dirty).unwrap_or(false) {
                    let _ = save_file(&mut self.open[idx]);
                }
                if let Some(file) = self.open.get(idx) {
                    self.waveforms.remove(&file.path);
                }
                self.open.remove(idx);
                self.active = if self.open.is_empty() {
                    None
                } else {
                    Some(idx.min(self.open.len() - 1))
                };
                self.reload_editor()
            }
            Message::EditorScrolled { offset_y, view_h } => {
                self.editor_scroll_y = offset_y.max(0.0);
                if view_h > 1.0 {
                    self.editor_view_h = view_h;
                }
                Task::none()
            }
            Message::EditorPage(direction) => self.page_cursor(direction),
            Message::EditorAction(action) => {
                if self.active_is_waveform() {
                    return Task::none();
                }
                if let text_editor::Action::Scroll { lines } = action {
                    return editor::scroll_by_lines(lines);
                }
                if action.is_edit() {
                    if let Some(i) = self.active {
                        if let Some(file) = self.open.get_mut(i) {
                            file.dirty = true;
                        }
                    }
                }
                let follow = matches!(
                    action,
                    text_editor::Action::Move(_)
                        | text_editor::Action::Select(_)
                        | text_editor::Action::Click(_)
                        | text_editor::Action::Edit(_)
                );
                self.editor_content.perform(action);
                self.sync_editor_to_active();
                if follow {
                    self.ensure_cursor_visible()
                } else {
                    Task::none()
                }
            }
            Message::Save => {
                self.menu_open = None;
                self.sync_editor_to_active();
                if let Some(i) = self.active {
                    match save_file(&mut self.open[i]) {
                        Ok(()) => {
                            let path = self.open[i].path.display().to_string();
                            self.status = format!("Saved {path}");
                            self.log(&format!("Saved {path}\n"));
                        }
                        Err(e) => self.log_err(&e),
                    }
                }
                Task::none()
            }
            Message::SaveAll => {
                self.menu_open = None;
                self.sync_editor_to_active();
                let mut errors = Vec::new();
                for f in &mut self.open {
                    if f.dirty {
                        if let Err(e) = save_file(f) {
                            errors.push(e);
                        }
                    }
                }
                for e in errors {
                    self.problems.push(e.clone());
                    self.log(&format!("ERROR: {e}\n"));
                }
                self.status = "Saved all".into();
                Task::none()
            }
            Message::ShowNewFile => {
                self.menu_open = None;
                self.dialog = Some(Dialog::NewFile {
                    name: "untitled.v".into(),
                });
                Task::none()
            }
            Message::ShowNewFolder => {
                self.menu_open = None;
                self.dialog = Some(Dialog::NewFolder {
                    name: "new_folder".into(),
                });
                Task::none()
            }
            Message::ShowFind => {
                self.menu_open = None;
                self.dialog = Some(Dialog::Find {
                    query: self.search_query.clone(),
                });
                Task::none()
            }
            Message::ShowAbout => {
                self.menu_open = None;
                self.dialog = Some(Dialog::About);
                Task::none()
            }
            Message::DialogInput(value) => {
                match &mut self.dialog {
                    Some(Dialog::NewFile { name }) => *name = value,
                    Some(Dialog::NewFolder { name }) => *name = value,
                    Some(Dialog::Find { query }) => *query = value,
                    _ => {}
                }
                Task::none()
            }
            Message::DialogConfirm => {
                let dialog = self.dialog.take();
                if let Some(dialog) = dialog {
                    match dialog {
                        Dialog::NewFile { name } => {
                            self.create_file(&name);
                            return editor::scroll_to_y(0.0);
                        }
                        Dialog::NewFolder { name } => self.create_folder(&name),
                        Dialog::Find { query } => {
                            self.search_query = query;
                            self.find_next();
                            return self.ensure_cursor_visible();
                        }
                        Dialog::About => {}
                    }
                }
                Task::none()
            }
            Message::DialogCancel => {
                self.dialog = None;
                Task::none()
            }
            Message::BottomTabSelected(tab) => {
                self.bottom = tab;
                Task::none()
            }
            Message::ToggleBottomPanel => {
                self.bottom_visible = !self.bottom_visible;
                Task::none()
            }
            Message::ClearBottom => {
                match self.bottom {
                    BottomTab::Console => self.console.clear(),
                    BottomTab::Problems => self.problems.clear(),
                }
                Task::none()
            }
            Message::RunSim => self.start_simulation(),
            Message::AnonymizeVcd => self.start_wave_tool(WaveToolKind::Anonymize),
            Message::VcdToSaif => self.start_wave_tool(WaveToolKind::ToSaif),
            Message::WaveToolFinished(result) => self.finish_wave_tool(result),
            Message::SimFinished(result) => {
                self.sim_running = false;
                self.bottom = BottomTab::Console;
                self.bottom_visible = true;
                if !result.log.is_empty() {
                    if !self.console.ends_with('\n') && !self.console.is_empty() {
                        self.console.push('\n');
                    }
                    self.console.push_str(&result.log);
                    if !result.log.ends_with('\n') {
                        self.console.push('\n');
                    }
                }
                if !result.ok {
                    self.problems
                        .push("xezim simulation failed. See OUTPUT.".into());
                    self.status = "Simulation failed".into();
                } else if let Some(vcd) = result.vcd {
                    self.status = format!("Wrote {}", vcd.display());
                    self.refresh_tree();
                    self.waveforms.remove(&vcd);
                    if self.view_mode == ViewMode::Waveform {
                        let load = self.open_path(&vcd);
                        return Task::batch([load, editor::scroll_to_y(0.0)]);
                    }
                } else {
                    self.status = "Simulation finished (no VCD)".into();
                    self.problems.push(
                        "Simulation finished but no .vcd was produced. Add $dumpfile / $dumpvars to the testbench."
                            .into(),
                    );
                }
                Task::none()
            }
            Message::SetViewMode(mode) => {
                self.menu_open = None;
                self.view_mode = mode;
                self.apply_view_mode_to_open_files()
            }
            Message::WaveNav(nav) => {
                if let Some(path) = self.active_path().cloned() {
                    if let Some(WaveStatus::Ready(wave)) = self.waveforms.get_mut(&path) {
                        wave.apply_nav(nav);
                    }
                }
                Task::none()
            }
            Message::WaveLoaded(path, result) => {
                match result {
                    Ok(wave) => {
                        let n = wave.trace_count();
                        self.log(&format!(
                            "Loaded waveform {} ({n} signals) with wellen (Surfer).\n",
                            path.display()
                        ));
                        self.waveforms
                            .insert(path.clone(), WaveStatus::Ready(wave));
                        self.status = format!("Waveform {} ({n} signals)", path.display());
                    }
                    Err(e) => {
                        self.waveforms
                            .insert(path.clone(), WaveStatus::Error(e.clone()));
                        self.log_err(&e);
                        if let Some(file) = self.open.iter_mut().find(|f| f.path == path) {
                            file.view = FileView::Text;
                        }
                        let reload = self.active_path() == Some(&path);
                        if reload {
                            return self.reload_editor();
                        }
                    }
                }
                Task::none()
            }
        }
    }

    fn start_simulation(&mut self) -> Task<Message> {
        self.menu_open = None;
        if self.sim_running || self.tool_running {
            return Task::none();
        }
        if self.dialog.is_some() {
            return Task::none();
        }
        self.sync_editor_to_active();
        let mut errors = Vec::new();
        for f in &mut self.open {
            if f.dirty {
                if let Err(e) = save_file(f) {
                    errors.push(e);
                }
            }
        }
        if !errors.is_empty() {
            for e in errors {
                self.problems.push(e.clone());
                self.log(&format!("ERROR: {e}\n"));
            }
            self.log_err("Save failed; simulation not started.");
            return Task::none();
        }

        let Some(root) = self.project_root() else {
            self.log_err("Open a folder first (File → Open Folder), then click Run.");
            return Task::none();
        };
        let active = self.active_path().cloned();
        match sim::prepare_job(&root, active.as_deref(), &self.open) {
            Ok(job) => {
                self.sim_running = true;
                self.bottom = BottomTab::Console;
                self.bottom_visible = true;
                self.status = "Running xezim…".into();
                self.log("Simulating with xezim (--wave) to generate a VCD…\n");
                self.log(&format!("{}\n", job.command_preview()));
                Task::perform(sim::run_job_async(job), Message::SimFinished)
            }
            Err(e) => {
                self.log_err(&e);
                Task::none()
            }
        }
    }

    fn start_wave_tool(&mut self, kind: WaveToolKind) -> Task<Message> {
        self.menu_open = None;
        if self.sim_running || self.tool_running {
            return Task::none();
        }
        let open: Vec<PathBuf> = self.open.iter().map(|file| file.path.clone()).collect();
        let input = match wave_tools::pick_input(
            self.active_path().map(PathBuf::as_path),
            &open,
            self.project_root().as_deref(),
            kind,
        ) {
            Ok(path) => path,
            Err(e) => {
                self.log_err(&e);
                return Task::none();
            }
        };

        self.tool_running = true;
        self.bottom = BottomTab::Console;
        self.bottom_visible = true;
        let (status, label) = match kind {
            WaveToolKind::Anonymize => ("Anonymizing VCD…", "Anon VCD"),
            WaveToolKind::ToSaif => ("Converting waveform to SAIF…", "To SAIF"),
        };
        self.status = status.into();
        self.log(&format!("{label}: {}\n", input.display()));
        Task::perform(wave_tools::run_async(kind, input), Message::WaveToolFinished)
    }

    fn finish_wave_tool(&mut self, result: Result<WaveToolOutput, String>) -> Task<Message> {
        self.tool_running = false;
        self.bottom = BottomTab::Console;
        self.bottom_visible = true;
        let output = match result {
            Ok(output) => output,
            Err(e) => {
                self.log_err(&e);
                return Task::none();
            }
        };
        self.log(&output.log);
        self.status = format!("Wrote {}", output.output.display());
        self.refresh_tree();
        if output.open_as_wave {
            self.view_mode = ViewMode::Waveform;
        }
        self.drop_open_path(&output.output);
        let load = self.open_path(&output.output);
        Task::batch([load, editor::scroll_to_y(0.0)])
    }

    fn drop_open_path(&mut self, path: &Path) {
        self.sync_editor_to_active();
        let Some(idx) = self.open.iter().position(|file| file.path == path) else {
            return;
        };
        self.waveforms.remove(path);
        self.open.remove(idx);
        self.active = match self.active {
            // The editor still shows this buffer. Leave no active file so the
            // next open does not copy that text into a different tab.
            Some(active) if active == idx => None,
            Some(active) if active > idx => Some(active - 1),
            other => other,
        };
    }

    fn refresh_tree(&mut self) {
        if let Some(project) = self.project.as_ref() {
            self.tree = Some(project.refresh_tree());
        }
    }

    fn open_folder_path(&mut self, path: PathBuf) {
        let folder = if path.is_dir() {
            path
        } else if let Some(parent) = path.parent() {
            self.log(&format!(
                "Selected file {}; opening parent folder {}.\n",
                path.display(),
                parent.display()
            ));
            parent.to_path_buf()
        } else {
            self.log_err(&format!("Not a folder: {}", path.display()));
            return;
        };

        if !folder.is_dir() {
            self.log_err(&format!("Not a folder: {}", folder.display()));
            return;
        }

        self.open_project(folder);
    }

    fn open_project(&mut self, root: PathBuf) {
        let project = IdeProject::new(root);
        self.log(&format!("Opened folder: {}\n", project.root.display()));
        let tree = project.build_tree();
        self.expanded.clear();
        self.expanded.insert(project.root.clone());
        let mut dirs = Vec::new();
        collect_dir_paths(&tree, &mut dirs);
        self.expanded.extend(dirs);
        self.tree = Some(tree);
        self.project = Some(project);
        self.open.clear();
        self.active = None;
        self.waveforms.clear();
        self.status = format!(
            "Opened folder: {}",
            self.project.as_ref().unwrap().root.display()
        );
        self.load_active_into_editor();

        if let Some(root) = self.project.as_ref().map(|p| p.root.clone()) {
            for path in initial_editor_files(&root) {
                let _ = self.open_path(&path);
            }
            if !self.open.is_empty() {
                self.active = Some(0);
                self.load_active_into_editor();
            }
        }
    }

    fn open_path(&mut self, path: &Path) -> Task<Message> {
        self.sync_editor_to_active();
        let want_wave = is_wave_path(path) && self.view_mode == ViewMode::Waveform;

        if let Some(idx) = self.open.iter().position(|f| f.path == path) {
            self.active = Some(idx);
            if let Some(file) = self.open.get_mut(idx) {
                file.view = if want_wave {
                    FileView::Waveform
                } else {
                    FileView::Text
                };
            }
            let load = if want_wave {
                self.ensure_wave_loaded(path)
            } else {
                Task::none()
            };
            self.load_active_into_editor();
            return load;
        }

        let mut file = match load_file(path) {
            Ok(file) => file,
            Err(e) if want_wave => OpenFile {
                path: path.to_path_buf(),
                content: String::new(),
                dirty: false,
                cursor: 0,
                view: FileView::Waveform,
            },
            Err(e) => {
                self.log_err(&e);
                self.problems.push(e);
                return Task::none();
            }
        };
        file.view = if want_wave {
            FileView::Waveform
        } else {
            FileView::Text
        };
        self.open.push(file);
        self.active = Some(self.open.len() - 1);
        self.status = format!("Opened {}", path.display());
        let load = if want_wave {
            self.ensure_wave_loaded(path)
        } else {
            Task::none()
        };
        self.load_active_into_editor();
        load
    }

    fn ensure_wave_loaded(&mut self, path: &Path) -> Task<Message> {
        let skip = matches!(
            self.waveforms.get(path),
            Some(WaveStatus::Ready(_)) | Some(WaveStatus::Loading)
        );
        if skip {
            Task::none()
        } else {
            self.waveforms
                .insert(path.to_path_buf(), WaveStatus::Loading);
            self.status = format!("Loading waveform {}…", path.display());
            Task::perform(waveform::load_wave_async(path.to_path_buf()), |(p, r)| {
                Message::WaveLoaded(p, r)
            })
        }
    }

    fn apply_view_mode_to_open_files(&mut self) -> Task<Message> {
        self.sync_editor_to_active();
        let mode = self.view_mode;
        let paths: Vec<PathBuf> = self.open.iter().map(|f| f.path.clone()).collect();
        let mut tasks = Vec::new();
        for path in &paths {
            if !is_wave_path(path) {
                continue;
            }
            if let Some(file) = self.open.iter_mut().find(|f| f.path == *path) {
                file.view = match mode {
                    ViewMode::Waveform => FileView::Waveform,
                    ViewMode::TextEditor => FileView::Text,
                };
                if mode == ViewMode::TextEditor && file.content.is_empty() {
                    if let Ok(loaded) = load_file(path) {
                        file.content = loaded.content;
                    }
                }
            }
            if mode == ViewMode::Waveform {
                tasks.push(self.ensure_wave_loaded(path));
            }
        }
        self.load_active_into_editor();
        if tasks.is_empty() {
            if self.active_is_waveform() {
                Task::none()
            } else {
                editor::scroll_to_y(0.0)
            }
        } else {
            Task::batch(tasks)
        }
    }

    fn active_is_waveform(&self) -> bool {
        self.active
            .and_then(|i| self.open.get(i))
            .is_some_and(|f| f.view == FileView::Waveform)
    }

    fn sync_editor_to_active(&mut self) {
        if self.active_is_waveform() {
            return;
        }
        if let Some(i) = self.active {
            if let Some(file) = self.open.get_mut(i) {
                file.content = self.editor_content.text();
                let (line, col) = self.editor_content.cursor_position();
                file.cursor = cursor_from_line_col(&file.content, line, col);
            }
        }
    }

    fn load_active_into_editor(&mut self) {
        self.editor_scroll_y = 0.0;
        if self.active_is_waveform() {
            return;
        }
        if let Some(i) = self.active {
            if let Some(file) = self.open.get(i) {
                let cursor = file.cursor;
                let content = file.content.clone();
                self.editor_content = text_editor::Content::with_text(&content);
                let (line, _) = cursor_line_col(&content, cursor);
                let y = (line.saturating_sub(1)) as f32 * EDITOR_LINE_HEIGHT
                    + EDITOR_LINE_HEIGHT * 0.4;
                self.editor_content
                    .perform(text_editor::Action::Click(Point::new(2.0, y)));
            }
        } else {
            self.editor_content = text_editor::Content::new();
        }
    }

    fn reload_editor(&mut self) -> Task<Message> {
        if self.active_is_waveform() {
            self.load_active_into_editor();
            return Task::none();
        }
        self.load_active_into_editor();
        let (line, _) = self.editor_content.cursor_position();
        let y = (editor::line_top(line) - self.editor_view_h * 0.25).max(0.0);
        editor::scroll_to_y(y)
    }

    fn ensure_cursor_visible(&self) -> Task<Message> {
        let view_h = self.editor_view_h;
        if view_h <= EDITOR_LINE_HEIGHT {
            return Task::none();
        }
        let (line, _) = self.editor_content.cursor_position();
        let line_top = editor::line_top(line);
        let line_bot = line_top + EDITOR_LINE_HEIGHT;
        let view_top = self.editor_scroll_y;
        let view_bot = view_top + view_h;
        let margin = EDITOR_LINE_HEIGHT;
        let y = if line_top < view_top + margin {
            (line_top - margin).max(0.0)
        } else if line_bot + margin > view_bot {
            (line_bot + margin - view_h).max(0.0)
        } else {
            return Task::none();
        };
        editor::scroll_to_y(y)
    }

    fn page_cursor(&mut self, direction: i32) -> Task<Message> {
        let (line, _) = self.editor_content.cursor_position();
        let last = self.editor_content.line_count().saturating_sub(1);
        let page = ((self.editor_view_h / EDITOR_LINE_HEIGHT).floor() as i32 - 2).max(1);
        let target = (line as i32 + direction * page).clamp(0, last as i32) as usize;
        let y = target as f32 * EDITOR_LINE_HEIGHT + EDITOR_LINE_HEIGHT * 0.4;
        self.editor_content
            .perform(text_editor::Action::Click(Point::new(2.0, y)));
        self.sync_editor_to_active();
        self.ensure_cursor_visible()
    }

    fn log(&mut self, msg: &str) {
        self.console.push_str(msg);
    }

    fn log_err(&mut self, msg: &str) {
        self.console.push_str(&format!("ERROR: {msg}\n"));
        self.status = msg.to_string();
        self.problems.push(msg.to_string());
    }

    fn project_root(&self) -> Option<PathBuf> {
        self.project.as_ref().map(|p| p.root.clone())
    }

    fn create_file(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(root) = self.project_root() else {
            self.log_err("Open a folder first (File → Open Folder).");
            return;
        };
        let path = root.join(name);
        if path.exists() {
            self.log_err(&format!("Already exists: {}", path.display()));
            return;
        }
        let stem = {
            let lower = name.to_ascii_lowercase();
            lower
                .trim_end_matches(".sv")
                .trim_end_matches(".vh")
                .trim_end_matches(".v")
                .to_string()
        };
        let body = if templates::is_testbench_filename(name) {
            templates::testbench_template(&stem)
        } else if is_verilog_name(name) {
            templates::module_template(&stem)
        } else {
            String::new()
        };
        if let Err(e) = std::fs::write(&path, body) {
            self.log_err(&e.to_string());
            return;
        }
        self.refresh_tree();
        let _ = self.open_path(&path);
        self.log(&format!("Created file {}\n", path.display()));
    }

    fn create_folder(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(root) = self.project_root() else {
            self.log_err("Open a folder first (File → Open Folder).");
            return;
        };
        let path = root.join(name);
        if let Err(e) = std::fs::create_dir_all(&path) {
            self.log_err(&e.to_string());
            return;
        }
        self.expanded.insert(path.clone());
        self.refresh_tree();
        self.log(&format!("Created folder {}\n", path.display()));
    }

    fn find_next(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        if self.active_is_waveform() {
            self.status = "Switch to Text Editor to search in a dump file.".into();
            return;
        }
        self.sync_editor_to_active();
        let Some(i) = self.active else { return };
        let content = self.open[i].content.clone();
        let start = self.open[i].cursor.saturating_add(1).min(content.len());
        let found = content[start..]
            .find(&self.search_query)
            .map(|o| start + o)
            .or_else(|| content.find(&self.search_query));
        if let Some(pos) = found {
            self.open[i].cursor = pos;
            self.status = format!("Found at {pos}");
            self.load_active_into_editor();
        } else {
            self.status = "Not found".into();
        }
    }

    fn active_path(&self) -> Option<&PathBuf> {
        self.active
            .and_then(|i| self.open.get(i))
            .map(|f| &f.path)
    }

    fn view(&self) -> Element<'_, Message> {
        let main = row![
            self.view_activity_bar(),
            self.view_sidebar(),
            column![
                self.view_editor_column(),
                if self.bottom_visible {
                    Element::from(self.view_bottom_panel())
                } else {
                    Space::new(Length::Fill, Length::Fixed(0.0)).into()
                },
            ]
            .width(Fill)
            .spacing(0),
        ]
        .height(Fill)
        .spacing(0);

        let content = column![
            container(main)
                .width(Fill)
                .height(Fill)
                .style(|_| panel_style(BG_EDITOR)),
            self.view_status_bar(),
        ]
        .height(Fill)
        .spacing(0);

        let content_area: Element<'_, Message> = if let Some(menu) = self.menu_open {
            stack![
                content,
                mouse_area(Space::new(Fill, Fill)).on_press(Message::MenuClose),
                container(
                    row![
                        Space::new(Length::Fixed(menu_dropdown_left(menu)), Length::Shrink),
                        self.view_menu_dropdown(menu),
                    ]
                    .align_y(Alignment::Start),
                )
                .width(Fill)
                .height(Length::Shrink)
                .align_x(Alignment::Start)
                .align_y(Alignment::Start)
                .padding(Padding {
                    top: 2.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
            ]
            .height(Fill)
            .into()
        } else {
            content.into()
        };

        column![
            self.view_menu_bar(),
            content_area,
        ]
        .height(Fill)
        .spacing(0)
        .into()
    }

    fn view_menu_dropdown(&self, menu: TopMenu) -> Element<'_, Message> {
        match menu {
            TopMenu::File => menu_dropdown(column![
                menu_item("Open Folder…", Message::OpenProject),
                menu_item("Open File…", Message::OpenFilePicker),
                menu_item("New File…", Message::ShowNewFile),
                menu_item("New Folder…", Message::ShowNewFolder),
                menu_item("Save", Message::Save),
                menu_item("Save All", Message::SaveAll),
                menu_item("Close Folder", Message::CloseProject),
                menu_item("Open Half Adder", Message::CreateSample),
            ]),
            TopMenu::Edit => menu_dropdown(column![
                menu_item("Find…", Message::ShowFind),
                menu_item("Refresh Explorer", Message::RefreshExplorer),
            ]),
            TopMenu::View => menu_dropdown(column![
                menu_item_owned(
                    view_mode_label("Text Editor", self.view_mode == ViewMode::TextEditor),
                    Message::SetViewMode(ViewMode::TextEditor),
                ),
                menu_item_owned(
                    view_mode_label("Waveform", self.view_mode == ViewMode::Waveform),
                    Message::SetViewMode(ViewMode::Waveform),
                ),
            ]),
            TopMenu::Run => menu_dropdown(column![
                menu_item("Run Simulation (F5)", Message::RunSim),
                menu_item("Anonymize VCD", Message::AnonymizeVcd),
                menu_item("Convert VCD to SAIF", Message::VcdToSaif),
            ]),
            TopMenu::Help => {
                menu_dropdown(column![menu_item("About Verilog IDE", Message::ShowAbout)])
            }
        }
    }

    fn view_menu_bar(&self) -> Element<'_, Message> {
        let file_active = self.menu_open == Some(TopMenu::File);
        let edit_active = self.menu_open == Some(TopMenu::Edit);
        let view_active = self.menu_open == Some(TopMenu::View);
        let run_active = self.menu_open == Some(TopMenu::Run);
        let help_active = self.menu_open == Some(TopMenu::Help);

        container(
            row![
                menu_label("File", file_active, Message::MenuToggle(TopMenu::File)),
                menu_label("Edit", edit_active, Message::MenuToggle(TopMenu::Edit)),
                menu_label("View", view_active, Message::MenuToggle(TopMenu::View)),
                menu_label("Run", run_active, Message::MenuToggle(TopMenu::Run)),
                menu_label("Help", help_active, Message::MenuToggle(TopMenu::Help)),
                horizontal_space(),
                mode_toggle(self.view_mode),
                run_toolbar_button(self.sim_running, !self.tool_running),
                vcd_tool_button(
                    "Anon VCD",
                    !self.sim_running && !self.tool_running,
                    Message::AnonymizeVcd,
                ),
                vcd_tool_button(
                    "To SAIF",
                    !self.sim_running && !self.tool_running,
                    Message::VcdToSaif,
                ),
                text("Verilog IDE").size(13).color(ACCENT),
            ]
            .height(Fill)
            .spacing(2)
            .padding([0, 8])
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .height(Length::Fixed(MENU_HEIGHT))
        .style(|_| panel_style(BG_MENU))
        .into()
    }

    fn view_activity_bar(&self) -> Element<'_, Message> {
        container(
            column![
                container(text("V").size(18).color(ACCENT))
                    .width(Length::Fixed(36.0))
                    .height(Length::Fixed(36.0))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(|_| iced::widget::container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.08, 0.20, 0.24))),
                        border: Border {
                            color: ACCENT,
                            width: 1.0,
                            radius: 10.0.into(),
                        },
                        ..Default::default()
                    }),
                Space::new(Length::Fill, Length::Shrink),
            ]
            .height(Fill)
            .padding(10)
            .align_x(Alignment::Center),
        )
        .width(Length::Fixed(ACTIVITY_WIDTH))
        .height(Fill)
        .style(|_| panel_style(BG_ACTIVITY))
        .into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        let title = row![
            text("PROJECT").size(11).color(ACCENT),
            horizontal_space(),
            sidebar_action("New", Message::ShowNewFile),
            sidebar_action("Dir", Message::ShowNewFolder),
            sidebar_action("Sync", Message::RefreshExplorer),
        ]
        .align_y(Alignment::Center)
        .padding([8, 10]);

        let project_header: Element<'_, Message> = if let Some(p) = self.project.as_ref() {
            row![
                text("v").size(12).color(FG_MUTED),
                text(p.name.as_str()).size(13).color(FG_TEXT),
            ]
            .spacing(4)
            .padding([2, 8])
            .into()
        } else {
            Space::new(Length::Fill, Length::Fixed(0.0)).into()
        };

        let tree_body: Element<'_, Message> = if let Some(tree) = &self.tree {
            scrollable(
                column(self.render_tree_nodes(tree, 0))
                    .spacing(0)
                    .padding([0, 4]),
            )
            .height(Fill)
            .into()
        } else {
            scrollable(
                column![
                    text("No folder opened").size(13).color(FG_MUTED),
                    Space::new(Length::Fill, Length::Fixed(8.0)),
                    sidebar_link("Open half adder", Message::CreateSample),
                    sidebar_link("Open folder…", Message::OpenProject),
                    sidebar_link("Open file…", Message::OpenFilePicker),
                ]
                .padding(12)
                .spacing(6),
            )
            .height(Fill)
            .into()
        };

        container(
            column![title, horizontal_rule(1), project_header, tree_body]
                .spacing(0)
                .height(Fill),
        )
        .width(Length::Fixed(SIDEBAR_WIDTH))
        .height(Fill)
        .style(|_| panel_style(BG_SIDEBAR))
        .into()
    }

    fn render_tree_nodes(
        &self,
        node: &TreeNode,
        depth: usize,
    ) -> Vec<Element<'static, Message>> {
        let mut items = Vec::new();
        let indent = depth as f32 * 16.0;
        let is_active = self.active_path().is_some_and(|p| p == &node.path);

        if depth > 0 {
            if node.is_dir {
                let expanded = self.expanded.contains(&node.path);
                let chevron = if expanded { "▾" } else { "▸" };
                items.push(tree_row(
                    indent,
                    chevron,
                    node.name.clone(),
                    is_active,
                    Message::ToggleDir(node.path.clone()),
                ));
            } else {
                items.push(tree_row(
                    indent,
                    file_badge(&node.path),
                    node.name.clone(),
                    is_active,
                    Message::OpenFile(node.path.clone()),
                ));
            }
        }

        if node.is_dir && (depth == 0 || self.expanded.contains(&node.path)) {
            for child in &node.children {
                items.extend(self.render_tree_nodes(child, depth + 1));
            }
        }

        items
    }

    fn view_editor_column(&self) -> Element<'_, Message> {
        let editor = column![
            self.view_tab_bar(),
            container(self.view_editor_body())
                .width(Fill)
                .height(Fill)
                .style(|_| panel_style(BG_EDITOR)),
        ]
        .spacing(0)
        .height(Fill);

        if self.dialog.is_some() {
            column![editor, self.view_dialog_bar()].spacing(0).height(Fill).into()
        } else {
            editor.into()
        }
    }

    fn view_tab_bar(&self) -> Element<'_, Message> {
        if self.open.is_empty() {
            return container(Space::new(Length::Fill, Length::Fixed(TAB_HEIGHT - 4.0)))
                .width(Fill)
                .style(|_| panel_style(BG_TABS))
                .into();
        }

        let tabs = scrollable(
            row(
                self.open.iter().enumerate().map(|(i, f)| {
                    let name = f
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("untitled");
                    let mut title = String::new();
                    if f.dirty {
                        title.push_str("* ");
                    }
                    if f.view == FileView::Waveform {
                        title.push_str("~ ");
                    }
                    title.push_str(name);
                    let selected = self.active == Some(i);
                    row![
                        tab_button(title, selected, Message::SelectTab(i)),
                        button(text("×"))
                            .on_press(Message::CloseTab(i))
                            .padding([2, 6])
                            .style(|_, _| tab_close_style()),
                    ]
                    .spacing(0)
                    .into()
                }),
            )
            .spacing(0)
            .padding([0, 4]),
        )
        .direction(iced::widget::scrollable::Direction::Horizontal(
            Default::default(),
        ));

        container(tabs)
            .width(Fill)
            .height(Length::Fixed(TAB_HEIGHT))
            .style(|_| panel_style(BG_TABS))
            .into()
    }

    fn view_editor_body(&self) -> Element<'_, Message> {
        if self.open.is_empty() {
            return view_welcome();
        }

        if self.active_is_waveform() {
            return self.view_waveform_body();
        }

        let line_count = self.editor_content.line_count().max(1);
        let current_line = self.editor_content.cursor_position().0 + 1;
        let content_h = editor::content_height(line_count, self.editor_view_h);

        let editor = text_editor(&self.editor_content)
            .font(Font::MONOSPACE)
            .size(EDITOR_FONT_SIZE)
            .line_height(editor::line_height())
            .padding(EDITOR_PADDING)
            .wrapping(Wrapping::None)
            .height(Length::Fixed(content_h))
            .on_action(Message::EditorAction)
            .key_binding(|press| match &press.key {
                Key::Named(keyboard::key::Named::PageDown) => {
                    Some(text_editor::Binding::Custom(Message::EditorPage(1)))
                }
                Key::Named(keyboard::key::Named::PageUp) => {
                    Some(text_editor::Binding::Custom(Message::EditorPage(-1)))
                }
                Key::Named(keyboard::key::Named::F5) => {
                    Some(text_editor::Binding::Custom(Message::RunSim))
                }
                _ => text_editor::Binding::from_key_press(press),
            })
            .highlight_with::<VerilogHighlighter>(
                VerilogHighlightSettings {
                    enabled: self
                        .active_path()
                        .map(|path| verilog_highlighter::syntax_enabled_for_path(path.as_path()))
                        .unwrap_or(true),
                    rust: false,
                },
                verilog_highlighter::format_highlight,
            );

        let body = row![
            editor::line_gutter(line_count, current_line, self.editor_view_h),
            container(editor)
                .width(Fill)
                .height(Length::Fixed(content_h)),
        ]
        .spacing(0)
        .width(Fill)
        .height(Length::Fixed(content_h));

        container(
            scrollable(body)
                .id(editor::scroll_id())
                .width(Fill)
                .height(Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new(),
                ))
                .on_scroll(|viewport| Message::EditorScrolled {
                    offset_y: viewport.absolute_offset().y,
                    view_h: viewport.bounds().height,
                }),
        )
        .width(Fill)
        .height(Fill)
        .style(|_| panel_style(BG_EDITOR))
        .into()
    }

    fn view_waveform_body(&self) -> Element<'_, Message> {
        let path = match self.active_path() {
            Some(p) => p,
            None => {
                return container(text("No waveform").color(FG_MUTED))
                    .width(Fill)
                    .height(Fill)
                    .center_x(Fill)
                    .center_y(Fill)
                    .into();
            }
        };

        let tools_enabled = !self.sim_running && !self.tool_running;
        let vcd_only = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("vcd"));
        let file_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("waveform")
            .to_string();
        let tool_bar = container(
            row![
                vcd_tool_button(
                    "Anon VCD",
                    tools_enabled && vcd_only,
                    Message::AnonymizeVcd,
                ),
                vcd_tool_button("To SAIF", tools_enabled, Message::VcdToSaif),
                horizontal_space(),
                text(file_label).size(12).color(FG_MUTED),
            ]
            .spacing(6)
            .padding([6, 8])
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .style(|_| panel_style(BG_TABS));

        let body: Element<'_, Message> = match self.waveforms.get(path) {
            Some(WaveStatus::Ready(wave)) => {
                let height = wave.content_height().max(self.editor_view_h);
                scrollable(
                    container(waveform::canvas(wave, Message::WaveNav))
                        .width(Fill)
                        .height(Length::Fixed(height)),
                )
                .width(Fill)
                .height(Fill)
                .into()
            }
            Some(WaveStatus::Loading) | None => container(
                column![
                    text("Loading waveform…").size(16).color(FG_TEXT),
                    text("Parsed with wellen (Surfer).")
                        .size(13)
                        .color(FG_MUTED),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into(),
            Some(WaveStatus::Error(e)) => container(
                column![
                    text("Could not open waveform").size(16).color(FG_TEXT),
                    text(e.as_str()).size(13).color(FG_MUTED),
                    Space::new(Length::Shrink, Length::Fixed(8.0)),
                    welcome_button(
                        "Open as text",
                        Message::SetViewMode(ViewMode::TextEditor),
                        true,
                    ),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into(),
        };

        container(column![tool_bar, body].spacing(0).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(|_| panel_style(BG_EDITOR))
            .into()
    }

    fn view_bottom_panel(&self) -> Element<'_, Message> {
        let problems_label = format!("PROBLEMS ({})", self.problems.len());
        let panel_tabs = row![
            panel_tab(
                "OUTPUT".into(),
                self.bottom == BottomTab::Console,
                Message::BottomTabSelected(BottomTab::Console),
            ),
            panel_tab(
                problems_label,
                self.bottom == BottomTab::Problems,
                Message::BottomTabSelected(BottomTab::Problems),
            ),
            horizontal_space(),
            button(text("^")).on_press(Message::ToggleBottomPanel),
            button(text("Clear")).on_press(Message::ClearBottom),
        ]
        .spacing(4)
        .padding([4, 8])
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.bottom {
            BottomTab::Console => scrollable(
                text(self.console.as_str())
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(FG_TEXT),
            )
            .height(Fill)
            .into(),
            BottomTab::Problems => {
                if self.problems.is_empty() {
                    text("No problems detected.").size(12).color(FG_MUTED).into()
                } else {
                    scrollable(
                        column(
                            self.problems
                                .iter()
                                .enumerate()
                                .map(|(i, p)| {
                                    text(format!("{}. {p}", i + 1))
                                        .size(12)
                                        .color(FG_TEXT)
                                        .into()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .spacing(2)
                        .padding(8),
                    )
                    .height(Fill)
                    .into()
                }
            }
        };

        container(column![panel_tabs, horizontal_rule(1), body].spacing(0).height(Fill))
            .width(Fill)
            .height(Length::Fixed(BOTTOM_HEIGHT))
            .style(|_| panel_style(BG_SIDEBAR))
            .into()
    }

    fn view_status_bar(&self) -> Element<'_, Message> {
        let branch = self
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "No Folder".into());

        let detail = if let Some(i) = self.active {
            if let Some(f) = self.open.get(i) {
                if f.view == FileView::Waveform {
                    let extra = self
                        .waveforms
                        .get(&f.path)
                        .and_then(|s| match s {
                            WaveStatus::Ready(w) => Some(w.cursor_label().unwrap_or_else(|| {
                                w.time_span_label()
                            })),
                            WaveStatus::Loading => Some("loading…".into()),
                            WaveStatus::Error(_) => Some("error".into()),
                        })
                        .unwrap_or_else(|| "waveform".into());
                    format!("{extra}   {}", f.path.display())
                } else {
                    let (line, col) = cursor_line_col(&f.content, f.cursor);
                    let dirty = if f.dirty { " *" } else { "" };
                    format!(
                        "Ln {line}, Col {col}{dirty}   {}",
                        f.path.display()
                    )
                }
            } else {
                self.status.clone()
            }
        } else {
            self.status.clone()
        };

        let sim = if self.sim_running {
            "Simulating"
        } else if self.tool_running {
            "Working"
        } else {
            "xezim"
        };

        container(
            row![
                text(branch).size(12).color(Color::WHITE),
                text("   ").size(12),
                text(sim).size(12).color(ACCENT),
                horizontal_space(),
                text(detail).size(12).color(Color::WHITE),
            ]
            .padding([2, 12])
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .height(Length::Fixed(STATUS_HEIGHT))
        .style(|_| panel_style(BG_STATUS))
        .into()
    }

    fn view_dialog_bar(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.dialog else {
            return Space::new(Length::Shrink, Length::Fixed(0.0)).into();
        };

        let (title, input, show_input) = match dialog {
            Dialog::NewFile { name } => ("New File", name.clone(), true),
            Dialog::NewFolder { name } => ("New Folder", name.clone(), true),
            Dialog::Find { query } => ("Find", query.clone(), true),
            Dialog::About => ("About", String::new(), false),
        };

        let body: Element<'_, Message> = if show_input {
            column![
                text(title).size(14).color(FG_TEXT),
                text_input("Name…", &input).on_input(Message::DialogInput),
                row![
                    button("OK").on_press(Message::DialogConfirm),
                    button("Cancel").on_press(Message::DialogCancel),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into()
        } else {
            column![
                text("Verilog IDE").size(18).color(FG_TEXT),
                text("A desktop editor for Verilog and Verilog testbenches.").color(FG_MUTED),
                text("Run uses xezim and writes a .vcd. The half-adder sample is half_adder.v and half_adder_tb.v.").color(FG_TEXT),
                text("Anon VCD flattens a dump. To SAIF writes switching activity. View → Waveform draws traces with wellen.").color(FG_TEXT),
                button("Close").on_press(Message::DialogCancel).style(|_, status| {
                    let hovered = matches!(
                        status,
                        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                    );
                    accent_button_style(true, hovered)
                }),
            ]
            .spacing(6)
            .into()
        };

        container(body)
            .padding(12)
            .width(Fill)
            .style(|_| dialog_style())
            .into()
    }
}

fn is_verilog_name(name: &str) -> bool {
    name.ends_with(".v") || name.ends_with(".sv")
}

fn panel_style(bg: Color) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: BORDER,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn dialog_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(iced::Background::Color(Color::from_rgb(0.11, 0.15, 0.19))),
        border: Border {
            color: ACCENT,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    }
}

fn tab_close_style() -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: None,
        text_color: FG_MUTED,
        ..Default::default()
    }
}

fn menu_label(label: &'static str, active: bool, msg: Message) -> Element<'static, Message> {
    button(
        text(label)
            .size(13)
            .color(FG_TEXT),
    )
    .on_press(msg)
    .padding([6, 12])
    .style(move |_, status| {
        let hovered = matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
        iced::widget::button::Style {
            background: if active {
                Some(iced::Background::Color(Color::from_rgb(0.30, 0.30, 0.32)))
            } else if hovered {
                Some(iced::Background::Color(Color::from_rgb(0.26, 0.26, 0.28)))
            } else {
                None
            },
            text_color: FG_TEXT,
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

fn menu_item(label: &'static str, msg: Message) -> Element<'static, Message> {
    menu_item_owned(label.to_string(), msg)
}

fn menu_item_owned(label: String, msg: Message) -> Element<'static, Message> {
    button(text(label).size(13))
        .on_press(msg)
        .width(Fill)
        .padding([7, 12])
        .style(|_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: if hovered {
                    Some(iced::Background::Color(Color::from_rgb(0.12, 0.26, 0.30)))
                } else {
                    None
                },
                text_color: FG_TEXT,
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

fn view_mode_label(name: &str, selected: bool) -> String {
    if selected {
        format!("●  {name}")
    } else {
        format!("   {name}")
    }
}

fn menu_dropdown_left(menu: TopMenu) -> f32 {
    match menu {
        TopMenu::File => 8.0,
        TopMenu::Edit => 52.0,
        TopMenu::View => 96.0,
        TopMenu::Run => 148.0,
        TopMenu::Help => 192.0,
    }
}

fn mode_toggle(mode: ViewMode) -> Element<'static, Message> {
    row![
        mode_chip("Text", mode == ViewMode::TextEditor, Message::SetViewMode(ViewMode::TextEditor)),
        mode_chip("Wave", mode == ViewMode::Waveform, Message::SetViewMode(ViewMode::Waveform)),
    ]
    .spacing(4)
    .into()
}

fn mode_chip(label: &'static str, selected: bool, msg: Message) -> Element<'static, Message> {
    button(text(label).size(12))
        .on_press(msg)
        .padding([4, 10])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: Some(iced::Background::Color(if selected {
                    Color::from_rgb(0.10, 0.36, 0.40)
                } else if hovered {
                    Color::from_rgb(0.14, 0.18, 0.22)
                } else {
                    Color::from_rgb(0.10, 0.13, 0.17)
                })),
                text_color: if selected { Color::WHITE } else { FG_TEXT },
                border: Border {
                    color: if selected { ACCENT } else { BORDER },
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn run_toolbar_button(running: bool, enabled: bool) -> Element<'static, Message> {
    let label = if running { "Running…" } else { "Run" };
    let mut btn = button(text(label).size(13)).padding([5, 16]).style(move |_, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            background: Some(iced::Background::Color(if running {
                Color::from_rgb(0.12, 0.40, 0.30)
            } else if hovered {
                Color::from_rgb(0.20, 0.68, 0.50)
            } else {
                RUN_GREEN
            })),
            text_color: Color::WHITE,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });
    if !running && enabled {
        btn = btn.on_press(Message::RunSim);
    }
    btn.into()
}

fn vcd_tool_button(label: &'static str, enabled: bool, msg: Message) -> Element<'static, Message> {
    let mut btn = button(text(label).size(12)).padding([4, 10]).style(move |_, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            background: Some(iced::Background::Color(if !enabled {
                Color::from_rgb(0.12, 0.16, 0.20)
            } else if hovered {
                Color::from_rgb(0.16, 0.42, 0.48)
            } else {
                Color::from_rgb(0.12, 0.30, 0.36)
            })),
            text_color: if enabled { Color::WHITE } else { FG_MUTED },
            border: Border {
                color: if enabled { ACCENT } else { BORDER },
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    });
    if enabled {
        btn = btn.on_press(msg);
    }
    btn.into()
}

fn menu_dropdown(items: Column<'_, Message>) -> Element<'_, Message> {
    container(items.padding(6).spacing(2))
        .width(Length::Fixed(240.0))
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.10, 0.13, 0.17))),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 8.0,
            },
            ..Default::default()
        })
        .into()
}

fn sidebar_action(icon: &'static str, msg: Message) -> Element<'static, Message> {
    button(text(icon).size(11))
        .on_press(msg)
        .padding([3, 6])
        .style(|_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: if hovered {
                    Some(iced::Background::Color(Color::from_rgb(0.12, 0.22, 0.26)))
                } else {
                    None
                },
                text_color: if hovered { ACCENT } else { FG_MUTED },
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

fn sidebar_link(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label).size(13))
        .on_press(msg)
        .padding([4, 0])
        .style(|_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: None,
                text_color: if hovered { Color::WHITE } else { ACCENT },
                ..Default::default()
            }
        })
        .into()
}

fn file_badge(path: &Path) -> &'static str {
    if is_wave_path(path) {
        return "~";
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if templates::is_testbench_filename(name) {
        return "TB";
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".v")
        || lower.ends_with(".sv")
        || lower.ends_with(".vh")
        || lower.ends_with(".svh")
    {
        "V"
    } else {
        "·"
    }
}

fn tree_row(
    indent: f32,
    badge: &str,
    label: String,
    active: bool,
    msg: Message,
) -> Element<'static, Message> {
    let badge_color = if badge == "TB" {
        AMBER
    } else if badge == "~" || badge == "V" {
        ACCENT
    } else {
        FG_MUTED
    };
    row![
        Space::new(Length::Fixed(indent), Length::Shrink),
        button(
            row![
                container(text(badge.to_string()).size(10).color(if active {
                    Color::WHITE
                } else {
                    badge_color
                }))
                .width(Length::Fixed(22.0)),
                text(label).size(12).color(if active { Color::WHITE } else { FG_TEXT }),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .on_press(msg)
        .width(Fill)
        .padding([4, 6])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: if active {
                    Some(iced::Background::Color(Color::from_rgb(0.10, 0.34, 0.38)))
                } else if hovered {
                    Some(iced::Background::Color(Color::from_rgb(0.12, 0.16, 0.20)))
                } else {
                    None
                },
                text_color: FG_TEXT,
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }),
    ]
    .width(Fill)
    .into()
}

fn tab_button(title: String, selected: bool, msg: Message) -> Element<'static, Message> {
    button(text(title).size(12))
        .on_press(msg)
        .padding([8, 14])
        .style(move |_, _| iced::widget::button::Style {
            background: Some(iced::Background::Color(if selected {
                BG_TAB_ACTIVE
            } else {
                BG_TABS
            })),
            text_color: if selected { FG_TEXT } else { FG_MUTED },
            border: Border {
                color: if selected { ACCENT } else { Color::TRANSPARENT },
                width: if selected { 1.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn view_welcome() -> Element<'static, Message> {
    let mark = container(text("V").size(34).color(ACCENT))
        .width(Length::Fixed(76.0))
        .height(Length::Fixed(76.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(BG_SIDEBAR)),
            border: Border {
                color: ACCENT,
                width: 2.0,
                radius: 20.0.into(),
            },
            ..Default::default()
        });

    container(
        column![
            mark,
            Space::new(Length::Shrink, Length::Fixed(4.0)),
            text("Verilog IDE").size(36).color(FG_TEXT),
            text("Edit a module, run the testbench, and read the waveform.")
                .size(15)
                .color(FG_MUTED),
            Space::new(Length::Shrink, Length::Fixed(6.0)),
            row![
                welcome_button("Open half adder", Message::CreateSample, true),
                welcome_button("Open folder", Message::OpenProject, false),
                welcome_button("Open file", Message::OpenFilePicker, false),
            ]
            .spacing(10),
            Space::new(Length::Shrink, Length::Fixed(18.0)),
            welcome_step("1", "The half adder is half_adder.v plus half_adder_tb.v."),
            welcome_step("2", "Press Run or F5. xezim writes half_adder.vcd."),
            welcome_step("3", "Scroll to zoom the traces, drag to pan, click to place a cursor."),
        ]
        .align_x(Alignment::Center)
        .spacing(8)
        .width(Length::Fixed(560.0)),
    )
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .into()
}

fn welcome_step(n: &'static str, body: &'static str) -> Element<'static, Message> {
    row![
        container(text(n).size(12).color(Color::from_rgb(0.05, 0.08, 0.10)))
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(ACCENT)),
                border: Border {
                    radius: 11.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        text(body).size(13).color(FG_TEXT),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn welcome_button(label: &'static str, msg: Message, primary: bool) -> Element<'static, Message> {
    button(text(label).size(13))
        .on_press(msg)
        .padding([8, 16])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            accent_button_style(primary, hovered)
        })
        .into()
}

fn accent_button_style(primary: bool, hovered: bool) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(iced::Background::Color(if primary {
            if hovered {
                Color::from_rgb(0.38, 0.88, 0.90)
            } else {
                ACCENT
            }
        } else if hovered {
            Color::from_rgb(0.14, 0.20, 0.26)
        } else {
            BG_SIDEBAR
        })),
        text_color: if primary {
            Color::from_rgb(0.05, 0.08, 0.10)
        } else {
            FG_TEXT
        },
        border: Border {
            color: if primary { ACCENT } else { BORDER },
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn panel_tab(label: String, selected: bool, msg: Message) -> Element<'static, Message> {
    button(text(label).size(11))
        .on_press(msg)
        .padding([4, 8])
        .style(move |_, _| iced::widget::button::Style {
            background: if selected {
                Some(iced::Background::Color(BG_EDITOR))
            } else {
                None
            },
            text_color: if selected { FG_TEXT } else { FG_MUTED },
            ..Default::default()
        })
        .into()
}

async fn pick_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    let title = title.to_owned();

    #[cfg(target_os = "linux")]
    if zenity_available() {
        return tokio::task::spawn_blocking(move || pick_folder_zenity(&title))
            .await
            .map_err(|_| DialogError::Cancelled)?;
    }

    let picked = rfd::AsyncFileDialog::new()
        .set_title(&title)
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf());
    Ok(picked)
}

async fn pick_file(title: &str) -> Result<Option<PathBuf>, DialogError> {
    let title = title.to_owned();

    #[cfg(target_os = "linux")]
    if zenity_available() {
        return tokio::task::spawn_blocking(move || pick_file_zenity(&title))
            .await
            .map_err(|_| DialogError::Cancelled)?;
    }

    let picked = rfd::AsyncFileDialog::new()
        .set_title(&title)
        .add_filter("Verilog", &["v", "sv", "vh", "svh"])
        .add_filter("Text", &["txt", "md"])
        .add_filter("Waveform", &["vcd", "fst", "ghw"])
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf());
    Ok(picked)
}

#[cfg(target_os = "linux")]
fn zenity_available() -> bool {
    std::process::Command::new("zenity")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "linux")]
fn pick_folder_zenity(title: &str) -> Result<Option<PathBuf>, DialogError> {
    let output = std::process::Command::new("zenity")
        .args(["--file-selection", "--directory", "--title", title])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|_| DialogError::Cancelled)?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!path.is_empty()).then(|| PathBuf::from(path)))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(DialogError::Cancelled)
    }
}

#[cfg(target_os = "linux")]
fn pick_file_zenity(title: &str) -> Result<Option<PathBuf>, DialogError> {
    let output = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--title",
            title,
            "--file-filter",
            "Verilog | *.v *.sv *.vh *.svh",
            "--file-filter",
            "Waveform | *.vcd *.fst *.ghw",
            "--file-filter",
            "Text | *.txt *.md",
            "--file-filter",
            "All | *",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|_| DialogError::Cancelled)?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!path.is_empty()).then(|| PathBuf::from(path)))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(DialogError::Cancelled)
    }
}
