mod customization;
mod drivelist;
mod os_list;
mod writer;

use std::{error::Error, io};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
};
use reqwest::Client;
use tokio::sync::mpsc;

use crate::customization::{
    CustomizationOptions, CustomizationTab, CustomizationUiState, InputMode,
};
use crate::drivelist::Drive;
use crate::os_list::{Device, OsList, OsListItem};

enum AppMessage {
    OsListLoaded(Result<OsList, String>),
    WriteProgress(f64),
    VerifyProgress(f64),
    WriteStatus(String),
    WriteFinished,
    WriteError(String),
    WritingPhase(WritingPhase),
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum WritingPhase {
    Writing,
    Verifying,
}

#[derive(PartialEq, Clone, Copy)]
enum CurrentView {
    DeviceSelection,
    OsSelection,
    StorageSelection,
    Customization,
    WriteConfirmation,
    Writing,
    AbortConfirmation,
    Finished,
}

struct App {
    pub os_list: Option<OsList>,
    pub is_loading: bool,
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub list_state: ListState,
    pub navigation_stack: Vec<Vec<OsListItem>>,
    pub breadcrumbs: Vec<String>,
    pub selection_stack: Vec<usize>,
    pub current_view: CurrentView,
    pub drive_list: Vec<Drive>,
    pub drive_list_state: ListState,
    pub selected_os: Option<OsListItem>,
    pub selected_drive: Option<Drive>,
    pub write_progress: f64,
    pub verify_progress: f64,
    pub write_status: String,
    pub write_phase: Option<WritingPhase>,
    pub write_task: Option<tokio::task::JoinHandle<()>>,
    pub abort_handle: Option<tokio::task::AbortHandle>,

    // Customization
    pub customization_options: CustomizationOptions,
    pub customization_ui: CustomizationUiState,
    pub customization_menu_state: ListState,
    pub customization_sub_menu_state: ListState,
    pub in_customization_submenu: bool,

    // Device selection
    pub selected_device: Option<Device>,
    pub device_list_state: ListState,
    pub debug_mode: bool,
}

impl App {
    fn new() -> App {
        let debug_mode = std::env::args().any(|arg| arg == "--debug");
        App {
            os_list: None,
            is_loading: true,
            should_quit: false,
            error_message: None,
            list_state: ListState::default(),
            navigation_stack: Vec::new(),
            breadcrumbs: Vec::new(),
            selection_stack: Vec::new(),
            current_view: CurrentView::DeviceSelection,
            drive_list: Vec::new(),
            drive_list_state: ListState::default(),
            selected_os: None,
            selected_drive: None,
            write_progress: 0.0,
            verify_progress: 0.0,
            write_status: String::new(),
            write_phase: None,
            write_task: None,
            abort_handle: None,
            customization_options: CustomizationOptions::default(),
            customization_ui: CustomizationUiState::default(),
            customization_menu_state: ListState::default(),
            customization_sub_menu_state: ListState::default(),
            in_customization_submenu: false,
            selected_device: None,
            device_list_state: ListState::default(),
            debug_mode,
        }
    }

    fn customization_sub_item_count(&self) -> usize {
        match self.customization_menu_state.selected().unwrap_or(0) {
            0 => 1, // Hostname
            1 => 3, // Localization (Timezone, Keyboard, Locale)
            2 => 2, // User
            3 => 3, // Wi-Fi
            4 => 3, // Remote Access
            _ => 0,
        }
    }

    fn handle_customization_enter(&mut self) {
        let menu_idx = self.customization_menu_state.selected().unwrap_or(0);
        let sub_idx = self.customization_sub_menu_state.selected().unwrap_or(0);

        match menu_idx {
            0 => match sub_idx {
                // Hostname
                0 => self.start_editing(self.customization_options.hostname.clone()),
                _ => {}
            },
            1 => match sub_idx {
                // Localization
                0 => self.start_editing(self.customization_options.timezone.clone()),
                1 => self.start_editing(self.customization_options.keyboard_layout.clone()),
                2 => self.start_editing(self.customization_options.locale.clone()),
                _ => {}
            },
            2 => match sub_idx {
                // User
                0 => self.start_editing(self.customization_options.user_name.clone()),
                1 => self.start_editing(
                    self.customization_options
                        .password
                        .clone()
                        .unwrap_or_default(),
                ),
                _ => {}
            },
            3 => match sub_idx {
                // Wi-Fi
                0 => self.start_editing(self.customization_options.wifi_ssid.clone()),
                1 => self.start_editing(self.customization_options.wifi_password.clone()),
                2 => {
                    self.customization_options.wifi_hidden = !self.customization_options.wifi_hidden
                }
                _ => {}
            },
            4 => match sub_idx {
                // Remote Access
                0 => {
                    self.customization_options.ssh_enabled = !self.customization_options.ssh_enabled
                }
                1 => {
                    self.customization_options.ssh_password_auth =
                        !self.customization_options.ssh_password_auth
                }
                2 => self.start_editing(self.customization_options.ssh_public_keys.clone()),
                _ => {}
            },
            _ => {}
        }
    }

    fn start_editing(&mut self, current_value: String) {
        self.customization_ui.input_buffer = current_value;
        self.customization_ui.input_mode = InputMode::Editing;
    }

    fn apply_customization_edit(&mut self) {
        let menu_idx = self.customization_menu_state.selected().unwrap_or(0);
        let sub_idx = self.customization_sub_menu_state.selected().unwrap_or(0);
        let value = self.customization_ui.input_buffer.clone();

        match menu_idx {
            0 => match sub_idx {
                0 => self.customization_options.hostname = value,
                _ => {}
            },
            1 => match sub_idx {
                0 => self.customization_options.timezone = value,
                1 => self.customization_options.keyboard_layout = value,
                2 => self.customization_options.locale = value,
                _ => {}
            },
            2 => match sub_idx {
                0 => self.customization_options.user_name = value,
                1 => self.customization_options.password = Some(value),
                _ => {}
            },
            3 => match sub_idx {
                0 => self.customization_options.wifi_ssid = value,
                1 => self.customization_options.wifi_password = value,
                _ => {}
            },
            4 => match sub_idx {
                2 => self.customization_options.ssh_public_keys = value,
                _ => {}
            },
            _ => {}
        }
    }

    fn get_devices(&self) -> &[Device] {
        if let Some(os_list) = &self.os_list {
            &os_list.imager.devices
        } else {
            &[]
        }
    }

    fn next_device(&mut self) {
        let i = match self.device_list_state.selected() {
            Some(i) => {
                if i >= self.get_devices().len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.device_list_state.select(Some(i));
    }

    fn previous_device(&mut self) {
        let i = match self.device_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.get_devices().len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.device_list_state.select(Some(i));
    }

    fn select_device(&mut self) {
        if let Some(i) = self.device_list_state.selected() {
            if let Some(device) = self.get_devices().get(i) {
                self.selected_device = Some(device.clone());
                self.current_view = CurrentView::OsSelection;
                self.list_state.select(Some(0));
                // Reset OS navigation
                self.navigation_stack.clear();
                self.breadcrumbs.clear();
                self.selection_stack.clear();
            }
        }
    }

    fn current_items(&self) -> &[OsListItem] {
        if let Some(items) = self.navigation_stack.last() {
            items
        } else if let Some(os_list) = &self.os_list {
            &os_list.os_list
        } else {
            &[]
        }
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.current_items().len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.current_items().len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let item = self.current_items().get(i).cloned();
            if let Some(item) = item {
                if !item.subitems.is_empty() {
                    self.selection_stack.push(i);
                    self.navigation_stack.push(item.subitems);
                    self.breadcrumbs.push(item.name);
                    self.list_state.select(Some(0));
                } else {
                    self.selected_os = Some(item);
                    self.current_view = CurrentView::StorageSelection;
                    self.refresh_drives();
                }
            }
        }
    }

    fn refresh_drives(&mut self) {
        match crate::drivelist::get_drives() {
            Ok(drives) => {
                self.drive_list = drives.into_iter().filter(|d| !d.is_system()).collect();
                self.drive_list_state.select(Some(0));
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to list drives: {}", e));
            }
        }
    }

    fn select_drive(&mut self) {
        if let Some(i) = self.drive_list_state.selected() {
            if let Some(drive) = self.drive_list.get(i) {
                self.selected_drive = Some(drive.clone());
                self.current_view = CurrentView::Customization;
                self.customization_menu_state.select(Some(0));
            }
        }
    }

    fn next_drive(&mut self) {
        let i = match self.drive_list_state.selected() {
            Some(i) => {
                if i >= self.drive_list.len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.drive_list_state.select(Some(i));
    }

    fn previous_drive(&mut self) {
        let i = match self.drive_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.drive_list.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.drive_list_state.select(Some(i));
    }

    fn start_writing(&mut self, tx: mpsc::Sender<AppMessage>) {
        self.current_view = CurrentView::Writing;
        self.write_progress = 0.0;
        self.verify_progress = 0.0;
        self.write_phase = Some(WritingPhase::Writing);
        self.write_status = "Starting...".to_string();

        if let (Some(os), Some(drive)) = (self.selected_os.clone(), self.selected_drive.clone()) {
            let handle = tokio::spawn(async move {
                match crate::writer::write_image(os, drive, tx.clone()).await {
                    Ok(_) => {}
                    Err(e) => {
                        let _ = tx.send(AppMessage::WriteError(e.to_string())).await;
                    }
                }
            });
            self.abort_handle = Some(handle.abort_handle());
            self.write_task = Some(handle);
        }
    }

    fn abort_writing(&mut self) {
        if let Some(handle) = &self.abort_handle {
            handle.abort();
        }
        self.abort_handle = None;
        self.write_task = None;
        self.current_view = CurrentView::Finished;
        self.write_status = "Aborted".to_string();
        self.error_message = Some("Operation cancelled by user.".to_string());
    }

    fn back(&mut self) {
        if !self.navigation_stack.is_empty() {
            self.navigation_stack.pop();
            self.breadcrumbs.pop();
            let index = self.selection_stack.pop().unwrap_or(0);
            self.list_state.select(Some(index));
        } else {
            // Go back to device selection if stack is empty
            self.current_view = CurrentView::DeviceSelection;
            self.selected_os = None;
            self.breadcrumbs.clear();
            self.list_state.select(Some(0));
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create App
    let mut app = App::new();

    // Create a channel to communicate between the async fetch and the sync UI loop
    let (tx, mut rx) = mpsc::channel::<AppMessage>(100);

    // Spawn the fetch task
    let tx_os = tx.clone();
    tokio::spawn(async move {
        // Try local file first
        let local_path = "os_list_imagingutility_v4.json";
        if let Ok(file) = std::fs::File::open(local_path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(data) = serde_json::from_reader(reader) {
                let _ = tx_os.send(AppMessage::OsListLoaded(Ok(data))).await;
                return;
            }
        }

        let client = Client::builder()
            .user_agent("rpi-imager-tui/0.1")
            .build()
            .unwrap_or_else(|_| Client::new());

        let url = "https://downloads.raspberrypi.com/os_list_imagingutility_v4.json";
        match client.get(url).send().await {
            Ok(resp) => match resp.json::<OsList>().await {
                Ok(data) => {
                    let _ = tx_os.send(AppMessage::OsListLoaded(Ok(data))).await;
                }
                Err(e) => {
                    let _ = tx_os
                        .send(AppMessage::OsListLoaded(Err(e.to_string())))
                        .await;
                }
            },
            Err(e) => {
                let _ = tx_os
                    .send(AppMessage::OsListLoaded(Err(e.to_string())))
                    .await;
            }
        }
    });

    // Run the application
    let res = run_app(&mut terminal, &mut app, &mut rx, tx).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: &mut mpsc::Receiver<AppMessage>,
    tx: mpsc::Sender<AppMessage>,
) -> io::Result<()> {
    loop {
        // Check for updates from fetch task or write task
        match rx.try_recv() {
            Ok(AppMessage::OsListLoaded(result)) => match result {
                Ok(data) => {
                    app.os_list = Some(data);
                    app.is_loading = false;
                    app.list_state.select(Some(0));
                    app.device_list_state.select(Some(0));
                }
                Err(msg) => {
                    app.error_message = Some(msg);
                    app.is_loading = false;
                }
            },
            Ok(AppMessage::WriteProgress(p)) => {
                app.write_progress = p;
            }
            Ok(AppMessage::VerifyProgress(p)) => {
                app.verify_progress = p;
            }
            Ok(AppMessage::WritingPhase(phase)) => {
                app.write_phase = Some(phase);
            }
            Ok(AppMessage::WriteStatus(msg)) => {
                app.write_status = msg;
            }
            Ok(AppMessage::WriteFinished) => {
                app.write_progress = 100.0;
                app.verify_progress = 100.0;
                app.write_status = "Finished".to_string();
                app.current_view = CurrentView::Finished;
                app.write_phase = None;
            }
            Ok(AppMessage::WriteError(err)) => {
                app.error_message = Some(err);
                app.current_view = CurrentView::StorageSelection;
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                // No messages
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Sender dropped without sending?
                if app.is_loading {
                    app.error_message = Some("Network task disconnected unexpectedly".to_string());
                    app.is_loading = false;
                }
            }
        }

        terminal.draw(|f| ui(f, app))?;

        // Poll for events
        // We use a timeout to ensure we keep checking the channel if no keys are pressed
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.error_message.is_some() {
                        app.error_message = None;
                        continue;
                    }
                    match app.current_view {
                        CurrentView::DeviceSelection => match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Down => app.next_device(),
                            KeyCode::Up => app.previous_device(),
                            KeyCode::Enter => app.select_device(),
                            _ => {}
                        },
                        CurrentView::OsSelection => match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Esc => {
                                if !app.navigation_stack.is_empty() {
                                    app.back();
                                } else {
                                    // Go back to device selection
                                    app.current_view = CurrentView::DeviceSelection;
                                    app.selected_os = None;
                                    app.breadcrumbs.clear();
                                }
                            }
                            KeyCode::Down => app.next(),
                            KeyCode::Up => app.previous(),
                            KeyCode::Enter => app.select(),
                            KeyCode::Left | KeyCode::Backspace => app.back(),
                            _ => {}
                        },
                        CurrentView::StorageSelection => match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Esc | KeyCode::Left | KeyCode::Backspace => {
                                app.current_view = CurrentView::OsSelection;
                                app.drive_list.clear();
                                app.selected_os = None;
                            }
                            KeyCode::Down => app.next_drive(),
                            KeyCode::Up => app.previous_drive(),
                            KeyCode::Enter => app.select_drive(),
                            KeyCode::Char('r') => app.refresh_drives(),
                            KeyCode::Char('o') => {
                                app.current_view = CurrentView::Customization;
                                app.customization_ui.current_tab = CustomizationTab::General;
                                app.customization_ui.selected_field_index = 0;
                            }
                            _ => {}
                        },
                        CurrentView::Customization => {
                            if app.customization_ui.input_mode == InputMode::Editing {
                                match key.code {
                                    KeyCode::Enter => {
                                        app.apply_customization_edit();
                                        app.customization_ui.input_mode = InputMode::Navigation;
                                    }
                                    KeyCode::Esc => {
                                        app.customization_ui.input_mode = InputMode::Navigation;
                                        app.customization_ui.input_buffer.clear();
                                    }
                                    KeyCode::Backspace => {
                                        app.customization_ui.input_buffer.pop();
                                    }
                                    KeyCode::Char(c) => {
                                        app.customization_ui.input_buffer.push(c);
                                    }
                                    _ => {}
                                }
                            } else if app.in_customization_submenu {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Left => {
                                        app.in_customization_submenu = false;
                                        app.customization_sub_menu_state.select(None);
                                    }
                                    KeyCode::Down => {
                                        let max_idx =
                                            app.customization_sub_item_count().saturating_sub(1);
                                        let i = match app.customization_sub_menu_state.selected() {
                                            Some(i) => {
                                                if i >= max_idx {
                                                    0
                                                } else {
                                                    i + 1
                                                }
                                            }
                                            None => 0,
                                        };
                                        app.customization_sub_menu_state.select(Some(i));
                                    }
                                    KeyCode::Up => {
                                        let max_idx =
                                            app.customization_sub_item_count().saturating_sub(1);
                                        let i = match app.customization_sub_menu_state.selected() {
                                            Some(i) => {
                                                if i == 0 {
                                                    max_idx
                                                } else {
                                                    i - 1
                                                }
                                            }
                                            None => 0,
                                        };
                                        app.customization_sub_menu_state.select(Some(i));
                                    }
                                    KeyCode::Enter | KeyCode::Char(' ') => {
                                        app.handle_customization_enter();
                                    }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => app.should_quit = true,
                                    KeyCode::Esc => {
                                        app.current_view = CurrentView::StorageSelection;
                                    }
                                    KeyCode::Down => {
                                        let i = match app.customization_menu_state.selected() {
                                            Some(i) => {
                                                if i >= 5 {
                                                    0
                                                } else {
                                                    i + 1
                                                }
                                            }
                                            None => 0,
                                        };
                                        app.customization_menu_state.select(Some(i));
                                    }
                                    KeyCode::Up => {
                                        let i = match app.customization_menu_state.selected() {
                                            Some(i) => {
                                                if i == 0 {
                                                    5
                                                } else {
                                                    i - 1
                                                }
                                            }
                                            None => 0,
                                        };
                                        app.customization_menu_state.select(Some(i));
                                    }
                                    KeyCode::Enter | KeyCode::Right => {
                                        if let Some(5) = app.customization_menu_state.selected() {
                                            // NEXT selected
                                            app.current_view = CurrentView::WriteConfirmation;
                                        } else {
                                            app.in_customization_submenu = true;
                                            app.customization_sub_menu_state.select(Some(0));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        CurrentView::WriteConfirmation => match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Esc => {
                                app.current_view = CurrentView::StorageSelection;
                                app.selected_drive = None;
                            }
                            KeyCode::Char('y') | KeyCode::Enter => app.start_writing(tx.clone()),
                            KeyCode::Char('n') => {
                                app.current_view = CurrentView::StorageSelection;
                                app.selected_drive = None;
                            }
                            _ => {}
                        },
                        CurrentView::Writing => {
                            if key.code == KeyCode::Esc {
                                app.current_view = CurrentView::AbortConfirmation;
                            }
                        }
                        CurrentView::AbortConfirmation => match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => app.abort_writing(),
                            KeyCode::Char('n') | KeyCode::Esc => {
                                app.current_view = CurrentView::Writing;
                            }
                            _ => {}
                        },
                        CurrentView::Finished => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                                // Reset navigation but keep OS list
                                app.current_view = CurrentView::DeviceSelection;
                                app.selected_os = None;
                                app.selected_drive = None;
                                app.navigation_stack.clear();
                                app.breadcrumbs.clear();
                                app.list_state.select(Some(0));
                                app.selected_device = None;
                                app.device_list_state.select(Some(0));
                            }
                            _ => {}
                        },
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(5),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.area());

    let title_text = if app.debug_mode {
        "Raspberry Pi Imager TUI (DEBUG MODE)"
    } else {
        "Raspberry Pi Imager TUI"
    };

    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Magenta)),
        );
    f.render_widget(title, main_chunks[0]);

    // Footer: Description
    let description = match app.current_view {
        CurrentView::DeviceSelection => {
            if let Some(i) = app.device_list_state.selected() {
                app.get_devices()
                    .get(i)
                    .map(|d| d.description.as_str())
                    .unwrap_or("")
            } else {
                ""
            }
        }
        CurrentView::OsSelection => {
            if let Some(i) = app.list_state.selected() {
                app.current_items()
                    .get(i)
                    .map(|os| os.description.as_str())
                    .unwrap_or("")
            } else {
                ""
            }
        }
        CurrentView::StorageSelection => {
            if let Some(i) = app.drive_list_state.selected() {
                app.drive_list
                    .get(i)
                    .map(|d| d.description.as_str())
                    .unwrap_or("")
            } else {
                ""
            }
        }
        CurrentView::Customization => "Edit image customization options.",
        CurrentView::WriteConfirmation => "Confirm write operation.",
        CurrentView::Writing => app.write_status.as_str(),
        CurrentView::AbortConfirmation => match app.write_phase {
            Some(WritingPhase::Verifying) => "Skip verification?",
            _ => "Abort writing operation?",
        },
        CurrentView::Finished => "Write complete.",
    };

    let desc = Paragraph::new(description)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Description",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .style(Style::default().fg(Color::White))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(desc, main_chunks[2]);

    // Footer: Keys
    let keys = match app.current_view {
        CurrentView::DeviceSelection => "↑/↓: Navigate | Enter: Select | q: Quit",
        CurrentView::OsSelection => "↑/↓: Navigate | Enter: Select | Esc: Back | q: Quit",
        CurrentView::StorageSelection => {
            "↑/↓: Navigate | Enter: Select | o: Options | r: Refresh | Esc: Back | q: Quit"
        }
        CurrentView::Customization => {
            if app.customization_ui.input_mode == InputMode::Editing {
                "Enter: Save | Esc: Cancel"
            } else if app.in_customization_submenu {
                "Enter: Edit | Esc: Back to Menu"
            } else {
                "↑/↓: Navigate | Enter/→: Select | Esc: Back"
            }
        }
        CurrentView::WriteConfirmation => "y/Enter: Confirm | n/Esc: Cancel | q: Quit",
        CurrentView::Writing => "Esc: Cancel/Skip",
        CurrentView::AbortConfirmation => "y/Enter: Confirm | n/Esc: Continue",
        CurrentView::Finished => "Enter/Esc: Done | q: Quit",
    };
    let keys_para = Paragraph::new(keys).style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(keys_para, main_chunks[3]);

    if app.is_loading {
        let loading = Paragraph::new("Loading OS List from raspberrypi.com...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(loading, main_chunks[1]);
        return;
    } else if let Some(err) = &app.error_message {
        let error = Paragraph::new(format!("Error: {}", err))
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(error, main_chunks[1]);
        return;
    }

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(1)].as_ref())
        .split(main_chunks[1]);

    // Render Sidebar
    let steps = vec![
        ("Device", CurrentView::DeviceSelection),
        ("OS", CurrentView::OsSelection),
        ("Storage", CurrentView::StorageSelection),
        ("Customization", CurrentView::Customization),
        ("Writing", CurrentView::Writing),
        ("Done", CurrentView::Finished),
    ];

    let items: Vec<ListItem> = steps
        .iter()
        .map(|(label, view)| {
            let is_active = app.current_view == *view
                || (app.current_view == CurrentView::WriteConfirmation
                    && *label == "Customization");

            let style = if is_active {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", label), style)),
                Line::from(""),
            ])
        })
        .collect();

    let sidebar = List::new(items).block(
        Block::default()
            .borders(Borders::RIGHT)
            .title(" Setup Steps ")
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    f.render_widget(sidebar, content_chunks[0]);

    // Render Main Content
    match app.current_view {
        CurrentView::DeviceSelection => {
            let devices = app.get_devices();
            let items: Vec<ListItem> = devices
                .iter()
                .map(|d| {
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            d.name.clone(),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )),
                        Line::from(Span::styled(
                            d.description.clone(),
                            Style::default().fg(Color::Gray),
                        )),
                        Line::from(""),
                    ])
                })
                .collect();

            let list = List::new(items)
                .block(
                    Block::default().borders(Borders::ALL).title(Span::styled(
                        "Select your Raspberry Pi device",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Magenta)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, content_chunks[1], &mut app.device_list_state);
        }
        CurrentView::OsSelection => {
            let items: Vec<ListItem> = app
                .current_items()
                .iter()
                .map(|os| {
                    let title = if os.subitems.is_empty() {
                        os.name.clone()
                    } else {
                        format!("{} >", os.name)
                    };
                    ListItem::new(Line::from(Span::raw(title)))
                })
                .collect();

            let title = if app.breadcrumbs.is_empty() {
                "Operating Systems".to_string()
            } else {
                format!("Operating Systems > {}", app.breadcrumbs.join(" > "))
            };

            let list = List::new(items)
                .block(
                    Block::default().borders(Borders::ALL).title(Span::styled(
                        title,
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Magenta)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, content_chunks[1], &mut app.list_state);
        }
        CurrentView::StorageSelection => {
            let title = if let Some(os) = &app.selected_os {
                format!("Select Storage Device for {}", os.name)
            } else {
                "Select Storage Device".to_string()
            };

            let items: Vec<ListItem> = app
                .drive_list
                .iter()
                .map(|drive| {
                    let info = format!(
                        "{} - {} ({}){}",
                        drive.name,
                        drive.description,
                        if drive.removable {
                            "Removable"
                        } else {
                            "Fixed"
                        },
                        if drive.is_system() { " [SYSTEM]" } else { "" }
                    );
                    let style = if drive.is_system() {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(Line::from(Span::styled(info, style)))
                })
                .collect();

            let list = List::new(items)
                .block(
                    Block::default().borders(Borders::ALL).title(Span::styled(
                        title,
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Magenta)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, content_chunks[1], &mut app.drive_list_state);
        }
        CurrentView::Customization => {
            let area = content_chunks[1];
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(area);

            // Left Menu
            let menu_items_labels = vec![
                "Hostname",
                "Localization",
                "User",
                "Wi-Fi",
                "Remote Access",
                "NEXT >",
            ];
            let menu_items: Vec<ListItem> = menu_items_labels
                .iter()
                .map(|t| ListItem::new(Line::from(*t)))
                .collect();

            let menu_list = List::new(menu_items)
                .block(
                    Block::default()
                        .borders(Borders::RIGHT)
                        .title(" Options ")
                        .style(Style::default().fg(Color::White)),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Magenta)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");

            f.render_stateful_widget(menu_list, chunks[0], &mut app.customization_menu_state);

            // Right Content
            let opts = &app.customization_options;
            let mut items = Vec::new();
            let selected_menu = app.customization_menu_state.selected().unwrap_or(0);

            match selected_menu {
                0 => {
                    // Hostname
                    items.push(format!("Hostname: {}", opts.hostname));
                }
                1 => {
                    // Localization
                    items.push(format!("Timezone: {}", opts.timezone));
                    items.push(format!("Keyboard Layout: {}", opts.keyboard_layout));
                    items.push(format!("Locale: {}", opts.locale));
                }
                2 => {
                    // User
                    items.push(format!("Username: {}", opts.user_name));
                    items.push(format!(
                        "Password: {}",
                        opts.password.as_deref().unwrap_or("******")
                    ));
                }
                3 => {
                    // Wi-Fi
                    items.push(format!("SSID: {}", opts.wifi_ssid));
                    items.push(format!("Password: {}", opts.wifi_password));
                    items.push(format!(
                        "Hidden SSID: {}",
                        if opts.wifi_hidden { "[x]" } else { "[ ]" }
                    ));
                }
                4 => {
                    // Remote Access
                    items.push(format!(
                        "Enable SSH: {}",
                        if opts.ssh_enabled { "[x]" } else { "[ ]" }
                    ));
                    if opts.ssh_enabled {
                        items.push(format!(
                            "Password Auth: {}",
                            if opts.ssh_password_auth { "[x]" } else { "[ ]" }
                        ));
                    } else {
                        items.push("Password Auth: [ ]".to_string());
                    }
                    items.push(format!("Public Key: {}", opts.ssh_public_keys));
                }
                5 => {
                    // Next
                    items.push("Press Enter to proceed to writing.".to_string());
                }
                _ => {}
            }

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, val)| {
                    let mut content = val.clone();
                    if app.in_customization_submenu
                        && app.customization_sub_menu_state.selected() == Some(i)
                        && app.customization_ui.input_mode == InputMode::Editing
                    {
                        content = format!("> {}_", app.customization_ui.input_buffer);
                    }
                    ListItem::new(Line::from(content))
                })
                .collect();

            let content_block = Block::default()
                .borders(Borders::ALL)
                .title(" Settings ")
                .border_style(if app.in_customization_submenu {
                    if app.customization_ui.input_mode == InputMode::Editing {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Cyan)
                    }
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            let sub_list = List::new(list_items).block(content_block).highlight_style(
                if app.in_customization_submenu {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            );

            f.render_stateful_widget(sub_list, chunks[1], &mut app.customization_sub_menu_state);
        }
        CurrentView::WriteConfirmation => {
            let os_name = app
                .selected_os
                .as_ref()
                .map(|o| o.name.as_str())
                .unwrap_or("Unknown OS");
            let drive_name = app
                .selected_drive
                .as_ref()
                .map(|d| d.description.as_str())
                .unwrap_or("Unknown Drive");

            let text = vec![
                Line::from(Span::raw("Are you sure you want to write:")),
                Line::from(Span::styled(
                    os_name,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("to")),
                Line::from(Span::styled(
                    drive_name,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "This will erase all data on the drive!",
                    Style::default()
                        .fg(Color::Red)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "Press 'y' or Enter to continue, 'n' or Esc to cancel.",
                    Style::default().fg(Color::Yellow),
                )),
            ];

            let vertical_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(1),
                        Constraint::Length(10),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(content_chunks[1]);

            let horizontal_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(vertical_layout[1]);

            let p = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled(
                            "Confirm Write",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ))
                        .border_style(Style::default().fg(Color::Red)),
                )
                .style(Style::default().fg(Color::White))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(p, horizontal_layout[1]);
        }
        CurrentView::Writing => {
            let vertical_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(1),
                        Constraint::Length(3),
                        Constraint::Length(1),
                        Constraint::Length(3),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(content_chunks[1]);

            let horizontal_layout_write = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(vertical_layout[1]);

            let horizontal_layout_verify = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(vertical_layout[3]);

            let gauge_write = Gauge::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Writing...")
                        .border_style(Style::default().fg(Color::Green)),
                )
                .gauge_style(
                    Style::default()
                        .fg(Color::Green)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .percent(app.write_progress as u16)
                .label(format!("{:.1}%", app.write_progress));
            f.render_widget(gauge_write, horizontal_layout_write[1]);

            let gauge_verify = Gauge::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Verifying...")
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .gauge_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .percent(app.verify_progress as u16)
                .label(format!("{:.1}%", app.verify_progress));
            f.render_widget(gauge_verify, horizontal_layout_verify[1]);
        }
        CurrentView::AbortConfirmation => {
            let title = match app.write_phase {
                Some(WritingPhase::Verifying) => "Skip Verification",
                _ => "Abort Writing",
            };
            let message = match app.write_phase {
                Some(WritingPhase::Verifying) => "Are you sure you want to skip verification?",
                _ => {
                    "Are you sure you want to abort writing? This may leave the drive in an unusable state."
                }
            };

            let text = vec![
                Line::from(Span::styled(
                    title,
                    Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
                )),
                Line::from(""),
                Line::from(message),
                Line::from(""),
                Line::from(Span::raw(
                    "Press 'y' or Enter to confirm, 'n' or Esc to continue.",
                )),
            ];

            let vertical_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(1),
                        Constraint::Length(7),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(content_chunks[1]);

            let horizontal_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(vertical_layout[1]);

            let p = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Span::styled(
                            "Warning",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ))
                        .border_style(Style::default().fg(Color::Red)),
                )
                .style(Style::default().fg(Color::White))
                .alignment(ratatui::layout::Alignment::Center)
                .wrap(ratatui::widgets::Wrap { trim: true });
            f.render_widget(p, horizontal_layout[1]);
        }
        CurrentView::Finished => {
            let text = vec![
                Line::from(Span::styled(
                    "Write Successful!",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "You can now remove the SD card.",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "Press Enter to continue.",
                    Style::default().fg(Color::Gray),
                )),
            ];

            let vertical_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(1),
                        Constraint::Length(7),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(content_chunks[1]);

            let horizontal_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(vertical_layout[1]);

            let p = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Finished")
                        .border_style(Style::default().fg(Color::Green)),
                )
                .style(Style::default().fg(Color::White))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(p, horizontal_layout[1]);
        }
    }
}
