use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use iridium_stomp::{Connection, ConnectOptions, Frame};
use iridium_stomp::connection::AckMode;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Terminal,
};
use std::io::{self, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;

use super::state::{SharedState, new_shared_state};
use super::commands::{execute_command, CommandResult};
use super::args::Cli;

/// TUI Application
pub struct App {
    conn: Connection,
    state: SharedState,
    should_quit: bool,
}

impl App {
    fn new(conn: Connection, state: SharedState) -> Self {
        Self {
            conn,
            state,
            should_quit: false,
        }
    }
}

/// Run the CLI in TUI mode
pub async fn run(cli: &Cli) -> Result<(), (String, u8)> {
    // Parse heartbeat to get interval for state
    let hb_parts: Vec<&str> = cli.heartbeat.split(',').collect();
    let hb_interval = hb_parts.get(1)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(10000);

    // Create heartbeat notification channel
    let (hb_tx, mut hb_rx) = mpsc::channel::<()>(16);

    // Build connection options
    let options = ConnectOptions::default()
        .with_heartbeat_notify(hb_tx);

    let conn = Connection::connect_with_options(
        &cli.address,
        &cli.login,
        &cli.passcode,
        &cli.heartbeat,
        options,
    ).await.map_err(|e| super::plain::format_connection_error_pub(&e, &cli.address))?;

    // Create shared state
    let state = new_shared_state(
        cli.address.clone(),
        cli.login.clone(),
        hb_interval,
    );

    // Channel for new subscription requests
    let (sub_tx, mut sub_rx) = mpsc::channel::<String>(16);

    // Subscribe to requested destinations
    for dest in &cli.subscribe {
        subscribe_destination(&conn, dest, state.clone()).await?;
    }

    // Spawn heartbeat monitor task
    let state_hb = state.clone();
    tokio::spawn(async move {
        while hb_rx.recv().await.is_some() {
            let mut s = state_hb.lock().await;
            s.record_heartbeat();
        }
    });

    // Spawn task to handle new subscription requests
    let conn_sub = conn.clone();
    let state_sub = state.clone();
    tokio::spawn(async move {
        while let Some(dest) = sub_rx.recv().await {
            match subscribe_destination(&conn_sub, &dest, state_sub.clone()).await {
                Ok(()) => {
                    let mut s = state_sub.lock().await;
                    s.record_message("INFO", format!("Subscribed to {}", dest), vec![]);
                }
                Err((msg, _)) => {
                    let mut s = state_sub.lock().await;
                    s.record_message("ERROR", msg, vec![]);
                }
            }
        }
    });

    // Spawn task to monitor for ERROR frames from the broker
    let conn_err = conn.clone();
    let state_err = state.clone();
    tokio::spawn(async move {
        loop {
            match conn_err.next_frame().await {
                Some(iridium_stomp::ReceivedFrame::Error(err)) => {
                    let mut s = state_err.lock().await;
                    let msg = if let Some(ref body) = err.body {
                        format!("{}: {}", err.message, body)
                    } else {
                        err.message.clone()
                    };
                    // Include error frame headers for context when user toggles header display
                    s.record_message("BROKER ERROR", msg, err.frame.headers.clone());
                }
                Some(iridium_stomp::ReceivedFrame::Frame(_)) => {
                    // Other frames are handled by subscription receivers
                }
                None => break, // Connection closed
            }
        }
    });

    // Setup terminal
    enable_raw_mode().map_err(|e| (format!("Failed to enable raw mode: {}", e), 1))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| (format!("Failed to setup terminal: {}", e), 1))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| (format!("Failed to create terminal: {}", e), 1))?;

    // Create app
    let app = App::new(conn.clone(), state.clone());

    // Run the main loop
    let result = run_app(&mut terminal, app, &sub_tx).await;

    // Restore terminal
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    ).ok();
    terminal.show_cursor().ok();

    // Print summary if requested
    if cli.summary {
        let s = state.lock().await;
        println!("{}", s.generate_summary());
    }

    // Close connection
    conn.close().await;

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
    sub_tx: &mpsc::Sender<String>,
) -> Result<(), (String, u8)> {
    loop {
        // Draw UI
        {
            let state = app.state.lock().await;
            terminal.draw(|f| ui(f, &state))
                .map_err(|e| (format!("Draw error: {}", e), 1))?;
        }

        // Poll for events with timeout
        let has_event = event::poll(Duration::from_millis(100))
            .map_err(|e| (format!("Event poll error: {}", e), 1))?;

        if has_event {
            let evt = event::read()
                .map_err(|e| (format!("Event read error: {}", e), 1))?;

            if let Event::Key(key) = evt {
                match key.code {
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let mut state = app.state.lock().await;
                        state.toggle_headers();
                    }
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let mut state = app.state.lock().await;
                        if state.scroll_offset > 0 {
                            state.scroll_offset -= 1;
                        }
                    }
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let mut state = app.state.lock().await;
                        let max_scroll = state.messages.len().saturating_sub(1);
                        if state.scroll_offset < max_scroll {
                            state.scroll_offset += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        let mut state = app.state.lock().await;
                        state.scroll_offset = state.scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown => {
                        let mut state = app.state.lock().await;
                        let max_scroll = state.messages.len().saturating_sub(1);
                        state.scroll_offset = (state.scroll_offset + 10).min(max_scroll);
                    }
                    KeyCode::Up if key.modifiers.is_empty() => {
                        let mut state = app.state.lock().await;
                        state.history_prev();
                    }
                    KeyCode::Down if key.modifiers.is_empty() => {
                        let mut state = app.state.lock().await;
                        state.history_next();
                    }
                    KeyCode::Enter => {
                        let input = {
                            let mut state = app.state.lock().await;
                            let input = state.input.clone();
                            state.add_to_history(&input);
                            state.input.clear();
                            state.cursor_pos = 0;
                            input
                        };
                        if !input.is_empty() {
                            match execute_command(&input, &app.conn, app.state.clone(), sub_tx, true).await {
                                CommandResult::Ok => {}
                                CommandResult::Quit => {
                                    app.should_quit = true;
                                }
                                CommandResult::Info(msg) => {
                                    let mut state = app.state.lock().await;
                                    state.record_message("INFO", msg, vec![]);
                                }
                                CommandResult::Error(msg) => {
                                    let mut state = app.state.lock().await;
                                    state.record_message("ERROR", msg, vec![]);
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        let mut state = app.state.lock().await;
                        let pos = state.cursor_pos;
                        state.input.insert(pos, c);
                        state.cursor_pos += 1;
                    }
                    KeyCode::Backspace => {
                        let mut state = app.state.lock().await;
                        if state.cursor_pos > 0 {
                            state.cursor_pos -= 1;
                            let pos = state.cursor_pos;
                            state.input.remove(pos);
                        }
                    }
                    KeyCode::Delete => {
                        let mut state = app.state.lock().await;
                        let pos = state.cursor_pos;
                        if pos < state.input.len() {
                            state.input.remove(pos);
                        }
                    }
                    KeyCode::Left => {
                        let mut state = app.state.lock().await;
                        if state.cursor_pos > 0 {
                            state.cursor_pos -= 1;
                        }
                    }
                    KeyCode::Right => {
                        let mut state = app.state.lock().await;
                        if state.cursor_pos < state.input.len() {
                            state.cursor_pos += 1;
                        }
                    }
                    KeyCode::Home => {
                        let mut state = app.state.lock().await;
                        state.cursor_pos = 0;
                    }
                    KeyCode::End => {
                        let mut state = app.state.lock().await;
                        state.cursor_pos = state.input.len();
                    }
                    KeyCode::Esc => {
                        let mut state = app.state.lock().await;
                        state.input.clear();
                        state.cursor_pos = 0;
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

fn ui(f: &mut ratatui::Frame, state: &super::state::AppState) {
    let size = f.area();

    // Main layout: header, subscriptions, messages, input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(6 + state.subscriptions.len().min(5) as u16),  // Subscriptions
            Constraint::Min(5),     // Messages
            Constraint::Length(3),  // Input
        ])
        .split(size);

    // Header bar
    render_header(f, chunks[0], state);

    // Activity counts panel
    render_counts(f, chunks[1], state);

    // Messages panel
    render_messages(f, chunks[2], state);

    // Input bar
    render_input(f, chunks[3], state);
}

fn render_header(f: &mut ratatui::Frame, area: Rect, state: &super::state::AppState) {
    let (hb_indicator, is_pulsing) = state.heartbeat_indicator();
    let hb_secs = state.heartbeat_interval_ms / 1000;

    let hb_style = if is_pulsing {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else if hb_indicator == "!" {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let header_line = Line::from(vec![
        Span::raw(format!(" Host: {}    User: {}    Heartbeat: ", state.host, state.user)),
        Span::styled(hb_indicator, hb_style),
        Span::raw(format!(" ({}s)", hb_secs)),
    ]);

    let title = format!(" iridium-stomp ─── {} ", state.session_duration());

    let header = Paragraph::new(header_line)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(title));

    f.render_widget(header, area);
}

fn render_counts(f: &mut ratatui::Frame, area: Rect, state: &super::state::AppState) {
    let mut rows: Vec<Row> = Vec::new();

    // Add subscription counts (sorted by destination)
    let mut sorted_subs: Vec<_> = state.subscriptions.iter().collect();
    sorted_subs.sort_by(|a, b| a.0.cmp(b.0));
    for (dest, stats) in sorted_subs {
        rows.push(Row::new(vec![dest.clone(), stats.message_count.to_string()])
            .style(Style::default().fg(Color::Green)));
    }

    // Add other counts if non-zero
    if state.sent_count > 0 {
        rows.push(Row::new(vec!["Sent".to_string(), state.sent_count.to_string()])
            .style(Style::default().fg(Color::Blue)));
    }
    if state.info_count > 0 {
        rows.push(Row::new(vec!["Info".to_string(), state.info_count.to_string()])
            .style(Style::default().fg(Color::Cyan)));
    }
    if state.warning_count > 0 {
        rows.push(Row::new(vec!["Warnings".to_string(), state.warning_count.to_string()])
            .style(Style::default().fg(Color::Yellow)));
    }
    if state.error_count > 0 {
        rows.push(Row::new(vec!["Errors".to_string(), state.error_count.to_string()])
            .style(Style::default().fg(Color::Red)));
    }

    // Add total row
    let total = state.total_message_count() + state.sent_count + state.info_count + state.warning_count + state.error_count;
    if !rows.is_empty() {
        rows.push(Row::new(vec!["".to_string(), "─────────".to_string()]));
        rows.push(Row::new(vec!["Total".to_string(), total.to_string()])
            .style(Style::default().add_modifier(Modifier::BOLD)));
    }

    let widths = [Constraint::Percentage(80), Constraint::Percentage(20)];
    let table = Table::new(rows, widths)
        .header(Row::new(vec!["Activity", "Count"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(table, area);
}

fn render_messages(f: &mut ratatui::Frame, area: Rect, state: &super::state::AppState) {
    let header_hint = if state.show_headers { "[^H] hide headers" } else { "[^H] show headers" };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Messages {} ", header_hint));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Calculate visible messages
    let visible_height = inner.height as usize;
    let total_messages = state.messages.len();

    // Auto-scroll to bottom unless user has scrolled up
    let scroll_offset = if state.scroll_offset == 0 && total_messages > visible_height {
        total_messages.saturating_sub(visible_height)
    } else {
        state.scroll_offset
    };

    let mut lines: Vec<Line> = Vec::new();

    for (i, msg) in state.messages.iter().enumerate() {
        if i < scroll_offset {
            continue;
        }
        if lines.len() >= visible_height {
            break;
        }

        let time = msg.timestamp.format("%H:%M:%S").to_string();

        // Color and style based on message type
        let (dest_style, body_style, max_body_len) = match msg.destination.as_str() {
            "ERROR" | "BROKER ERROR" => (
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Red),
                200, // Show more of error messages
            ),
            "WARN" => (
                Style::default().fg(Color::Yellow),
                Style::default().fg(Color::Yellow),
                120,
            ),
            "INFO" => (
                Style::default().fg(Color::Cyan),
                Style::default().fg(Color::DarkGray),
                80,
            ),
            "SENT" => (
                Style::default().fg(Color::Blue),
                Style::default(),
                60,
            ),
            _ => (
                Style::default().fg(Color::Cyan),
                Style::default(),
                60,
            ),
        };

        let dest_display = if msg.destination.len() > 20 {
            format!("...{}", &msg.destination[msg.destination.len()-17..])
        } else {
            msg.destination.clone()
        };

        // Truncate body for display
        let body_display = if msg.body.len() > max_body_len {
            format!("{}...", &msg.body[..max_body_len - 3])
        } else {
            msg.body.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::raw(" ["),
            Span::styled(dest_display, dest_style),
            Span::raw("] "),
            Span::styled(body_display, body_style),
        ]));

        // Show headers if toggled
        if state.show_headers && !msg.headers.is_empty() {
            for (k, v) in &msg.headers {
                if lines.len() >= visible_height {
                    break;
                }
                let header_line = format!("    {}: {}", k, v);
                let truncated = if header_line.len() > 70 {
                    format!("{}...", &header_line[..67])
                } else {
                    header_line
                };
                lines.push(Line::from(vec![
                    Span::styled(truncated, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn render_input(f: &mut ratatui::Frame, area: Rect, state: &super::state::AppState) {
    let input_text = format!("> {}", state.input);

    let input = Paragraph::new(input_text.as_str())
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    f.render_widget(input, area);

    // Set cursor position
    let cursor_x = area.x + 3 + state.cursor_pos as u16;
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width - 1 {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Subscribe to a destination and spawn a message handler task
async fn subscribe_destination(
    conn: &Connection,
    dest: &str,
    state: SharedState,
) -> Result<(), (String, u8)> {
    let sub = conn
        .subscribe(dest, AckMode::Auto)
        .await
        .map_err(|e| (format!("Failed to subscribe to '{}': {}", dest, e), super::exit_codes::PROTOCOL_ERROR))?;

    // Register in state
    {
        let mut s = state.lock().await;
        s.register_subscription(dest);
    }

    // Spawn a task to receive incoming messages for this subscription
    let dest_clone = dest.to_string();
    let state_clone = state.clone();
    let mut rx = sub.into_receiver();
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            handle_message(&dest_clone, &frame, state_clone.clone()).await;
        }
    });

    Ok(())
}

/// Handle an incoming message
async fn handle_message(dest: &str, frame: &Frame, state: SharedState) {
    // Extract body
    let body = if frame.body.is_empty() {
        String::new()
    } else {
        match std::str::from_utf8(&frame.body) {
            Ok(s) => s.to_string(),
            Err(_) => format!("({} bytes, binary)", frame.body.len()),
        }
    };

    // Record in state
    let mut s = state.lock().await;
    s.record_message(dest, body, frame.headers.clone());
}
