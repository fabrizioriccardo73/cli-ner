use crate::report::operation_log::{read_recent_operations, ActionStatus, OperationRecord};
use crate::utils::format::format_bytes;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Cell, Paragraph, Row, Table,
        TableState, Tabs, Wrap,
    },
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io::{self, stdout};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Operations,
    OperationDetails,
    CategoryStats,
}

pub struct DashboardApp {
    pub records: Vec<OperationRecord>,
    pub selected_record_index: usize,
    pub table_state: TableState,
    pub details_table_state: TableState,
    pub active_tab: ActiveTab,
    pub should_quit: bool,
}

impl DashboardApp {
    pub fn new(records: Vec<OperationRecord>) -> Self {
        let mut table_state = TableState::default();
        if !records.is_empty() {
            table_state.select(Some(0));
        }

        let mut details_table_state = TableState::default();
        if let Some(first) = records.first() {
            if !first.items.is_empty() {
                details_table_state.select(Some(0));
            }
        }

        Self {
            records,
            selected_record_index: 0,
            table_state,
            details_table_state,
            active_tab: ActiveTab::Operations,
            should_quit: false,
        }
    }

    pub fn next_record(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let next = if self.selected_record_index + 1 >= self.records.len() {
            0
        } else {
            self.selected_record_index + 1
        };
        self.selected_record_index = next;
        self.table_state.select(Some(next));
        self.reset_details_selection();
    }

    pub fn previous_record(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let prev = if self.selected_record_index == 0 {
            self.records.len() - 1
        } else {
            self.selected_record_index - 1
        };
        self.selected_record_index = prev;
        self.table_state.select(Some(prev));
        self.reset_details_selection();
    }

    pub fn next_detail_item(&mut self) {
        if let Some(record) = self.records.get(self.selected_record_index) {
            if record.items.is_empty() {
                return;
            }
            let current = self.details_table_state.selected().unwrap_or(0);
            let next = if current + 1 >= record.items.len() {
                0
            } else {
                current + 1
            };
            self.details_table_state.select(Some(next));
        }
    }

    pub fn previous_detail_item(&mut self) {
        if let Some(record) = self.records.get(self.selected_record_index) {
            if record.items.is_empty() {
                return;
            }
            let current = self.details_table_state.selected().unwrap_or(0);
            let prev = if current == 0 {
                record.items.len() - 1
            } else {
                current - 1
            };
            self.details_table_state.select(Some(prev));
        }
    }

    fn reset_details_selection(&mut self) {
        if let Some(record) = self.records.get(self.selected_record_index) {
            if !record.items.is_empty() {
                self.details_table_state.select(Some(0));
            } else {
                self.details_table_state.select(None);
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Operations => ActiveTab::OperationDetails,
            ActiveTab::OperationDetails => ActiveTab::CategoryStats,
            ActiveTab::CategoryStats => ActiveTab::Operations,
        };
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Operations => ActiveTab::CategoryStats,
            ActiveTab::OperationDetails => ActiveTab::Operations,
            ActiveTab::CategoryStats => ActiveTab::OperationDetails,
        };
    }
}

pub fn run_dashboard(limit: usize) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let records = read_recent_operations(limit).unwrap_or_default();
    let mut app = DashboardApp::new(records);

    let res = main_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut DashboardApp,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Tab => {
                        app.next_tab();
                    }
                    KeyCode::BackTab => {
                        app.prev_tab();
                    }
                    KeyCode::Char('1') => {
                        app.active_tab = ActiveTab::Operations;
                    }
                    KeyCode::Char('2') => {
                        app.active_tab = ActiveTab::OperationDetails;
                    }
                    KeyCode::Char('3') => {
                        app.active_tab = ActiveTab::CategoryStats;
                    }
                    KeyCode::Down | KeyCode::Char('j') => match app.active_tab {
                        ActiveTab::Operations => app.next_record(),
                        ActiveTab::OperationDetails => app.next_detail_item(),
                        ActiveTab::CategoryStats => {}
                    },
                    KeyCode::Up | KeyCode::Char('k') => match app.active_tab {
                        ActiveTab::Operations => app.previous_record(),
                        ActiveTab::OperationDetails => app.previous_detail_item(),
                        ActiveTab::CategoryStats => {}
                    },
                    KeyCode::Enter | KeyCode::Char('d')
                        if app.active_tab == ActiveTab::Operations =>
                    {
                        app.active_tab = ActiveTab::OperationDetails;
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &mut DashboardApp) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header & Quick stats
            Constraint::Length(3), // Navigation Tabs
            Constraint::Min(10),   // Main content area
            Constraint::Length(2), // Footer shortcuts
        ])
        .split(size);

    // 1. Header
    render_header(f, chunks[0], app);

    // 2. Tabs
    render_tabs(f, chunks[1], app);

    // 3. Main Content based on active tab
    match app.active_tab {
        ActiveTab::Operations => render_operations_tab(f, chunks[2], app),
        ActiveTab::OperationDetails => render_details_tab(f, chunks[2], app),
        ActiveTab::CategoryStats => render_stats_tab(f, chunks[2], app),
    }

    // 4. Footer
    render_footer(f, chunks[3]);
}

fn render_header(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let total_freed: u64 = app
        .records
        .iter()
        .filter(|r| !r.dry_run)
        .map(|r| r.total_bytes_freed)
        .sum();
    let total_ops = app.records.len();
    let executed_ops = app.records.iter().filter(|r| !r.dry_run).count();

    let header_text = Line::from(vec![
        Span::styled(
            " 🧹 CLI-NER Audit Dashboard ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Total Reclaimed: "),
        Span::styled(
            format_bytes(total_freed),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Operations: "),
        Span::styled(
            format!("{total_ops} ({executed_ops} executed)"),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let header_paragraph = Paragraph::new(header_text)
        .block(header_block)
        .alignment(Alignment::Left);

    f.render_widget(header_paragraph, area);
}

fn render_tabs(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let titles = vec![
        " [1] Operations History ",
        " [2] Operation Details ",
        " [3] Category Statistics ",
    ];

    let selected_index = match app.active_tab {
        ActiveTab::Operations => 0,
        ActiveTab::OperationDetails => 1,
        ActiveTab::CategoryStats => 2,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(selected_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(symbols::DOT);

    f.render_widget(tabs, area);
}

fn render_operations_tab(f: &mut Frame, area: Rect, app: &mut DashboardApp) {
    if app.records.is_empty() {
        let empty_msg = Paragraph::new("No operation records found in ~/.cli-ner/logs/\nRun `cli-ner clean` to generate audit logs.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Operations History "));
        f.render_widget(empty_msg, area);
        return;
    }

    let header_cells = [
        "Timestamp (UTC)",
        "Command",
        "Category",
        "Mode",
        "Space Freed",
        "Items Count",
        "Duration",
    ]
    .into_iter()
    .map(|h| {
        Cell::from(h).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.records.iter().map(|rec| {
        let time_str = rec.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let mode_span = if rec.dry_run {
            Span::styled("DRY-RUN", Style::default().fg(Color::Yellow))
        } else {
            Span::styled(
                "EXECUTED",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let freed_span = if rec.total_bytes_freed > 0 {
            Span::styled(
                format_bytes(rec.total_bytes_freed),
                Style::default().fg(Color::Green),
            )
        } else {
            Span::raw("0 B")
        };

        let duration_str = if rec.duration_ms > 0 {
            format!("{} ms", rec.duration_ms)
        } else {
            "-".into()
        };

        let cells = vec![
            Cell::from(time_str),
            Cell::from(rec.command.clone()),
            Cell::from(rec.category.clone()),
            Cell::from(mode_span),
            Cell::from(freed_span),
            Cell::from(rec.total_items_count.to_string()),
            Cell::from(duration_str),
        ];

        Row::new(cells).height(1)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Length(15),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" 📜 Operations Log (Press [Enter] or [d] to inspect items) ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(30, 45, 65))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_details_tab(f: &mut Frame, area: Rect, app: &mut DashboardApp) {
    let current_record = match app.records.get(app.selected_record_index) {
        Some(r) => r,
        None => {
            let p = Paragraph::new("No record selected.")
                .block(Block::default().borders(Borders::ALL).title(" Details "));
            f.render_widget(p, area);
            return;
        }
    };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(8)])
        .split(area);

    // Summary of selected operation
    let mode_str = if current_record.dry_run {
        "DRY-RUN"
    } else {
        "EXECUTED"
    };
    let summary_text = vec![
        Line::from(vec![
            Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&current_record.id, Style::default().fg(Color::White)),
            Span::styled(" │ Time: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                current_record.timestamp.to_rfc3339(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" │ Category: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&current_record.category, Style::default().fg(Color::Yellow)),
            Span::styled(" │ Mode: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                mode_str,
                if current_record.dry_run {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Space Reclaimed: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(current_record.total_bytes_freed),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ Total Items: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                current_record.total_items_count.to_string(),
                Style::default().fg(Color::White),
            ),
            Span::styled(" │ Duration: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ms", current_record.duration_ms),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let summary_p = Paragraph::new(summary_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Selected Operation Overview "),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(summary_p, main_layout[0]);

    // Items table
    let header_cells = ["Target / Item Path", "Size", "Action Type", "Status"]
        .into_iter()
        .map(|h| {
            Cell::from(h).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = current_record.items.iter().map(|item| {
        let status_span = match &item.status {
            ActionStatus::Success => Span::styled("Success", Style::default().fg(Color::Green)),
            ActionStatus::Failed(e) => {
                Span::styled(format!("Failed: {e}"), Style::default().fg(Color::Red))
            }
            ActionStatus::Skipped(e) => {
                Span::styled(format!("Skipped: {e}"), Style::default().fg(Color::Yellow))
            }
        };

        let action_span = Span::styled(
            format!("{:?}", item.action),
            Style::default().fg(Color::Magenta),
        );

        let cells = vec![
            Cell::from(item.path.clone()),
            Cell::from(format_bytes(item.size_bytes)),
            Cell::from(action_span),
            Cell::from(status_span),
        ];

        Row::new(cells).height(1)
    });

    let items_table = Table::new(
        rows,
        [
            Constraint::Percentage(55),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Cleaned Items ({}) ", current_record.items.len()))
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(30, 45, 65))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ");

    f.render_stateful_widget(items_table, main_layout[1], &mut app.details_table_state);
}

fn render_stats_tab(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let mut category_sizes: HashMap<String, u64> = HashMap::new();
    let mut category_counts: HashMap<String, usize> = HashMap::new();

    for r in &app.records {
        if !r.dry_run {
            for item in &r.items {
                if matches!(item.status, ActionStatus::Success) {
                    *category_sizes.entry(r.category.clone()).or_insert(0) += item.size_bytes;
                    *category_counts.entry(r.category.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Breakdown Table
    let header_cells = ["Category", "Total Space Reclaimed", "Cleaned Count"]
        .into_iter()
        .map(|h| {
            Cell::from(h).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let mut cat_entries: Vec<(String, u64, usize)> = category_sizes
        .into_iter()
        .map(|(cat, size)| {
            let count = category_counts.get(&cat).copied().unwrap_or(0);
            (cat, size, count)
        })
        .collect();

    cat_entries.sort_by_key(|b| std::cmp::Reverse(b.1));

    let rows = cat_entries.iter().map(|(cat, size, count)| {
        let cells = vec![
            Cell::from(cat.clone()).style(Style::default().fg(Color::Yellow)),
            Cell::from(format_bytes(*size)).style(Style::default().fg(Color::Green)),
            Cell::from(count.to_string()),
        ];
        Row::new(cells).height(1)
    });

    let cat_table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Reclaimed Space by Category ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(cat_table, chunks[0]);

    // Right: Bar Chart representation
    let mb_bars: Vec<Bar> = cat_entries
        .iter()
        .take(6)
        .map(|(cat, size, _)| {
            let mb = *size / (1024 * 1024);
            Bar::default()
                .label(Line::from(cat.as_str()))
                .value(mb)
                .style(Style::default().fg(Color::LightGreen))
                .value_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
        })
        .collect();

    let bar_group = BarGroup::default().bars(&mb_bars);

    let barchart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Space Reclaimed (MB) ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .data(bar_group)
        .bar_width(8)
        .bar_gap(2)
        .max(
            cat_entries
                .first()
                .map(|(_, s, _)| *s / (1024 * 1024))
                .unwrap_or(100),
        );

    f.render_widget(barchart, chunks[1]);
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer_text = Line::from(vec![
        Span::styled(" [q] ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" Quit  "),
        Span::styled(" [Tab] ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" Switch Tab  "),
        Span::styled(
            " [1/2/3] ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(" Direct Tab  "),
        Span::styled(
            " [↑/↓] / [j/k] ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(" Select Item  "),
        Span::styled(
            " [Enter/d] ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(" View Details "),
    ]);

    let footer_p = Paragraph::new(footer_text).alignment(Alignment::Center);
    f.render_widget(footer_p, area);
}
