use crate::domain::session::Session;
use crate::services::ssh::SshCommand;
use crate::terminal::{DEFAULT_BG, DEFAULT_FG};
use eframe::egui;

// i dislike how this terminal works currently and it needs to be rewritten
// to allow users to have more control over the terminal styling.
// TODO: review how to improve user control over the terminal styling, possibly integrating it into the existing
// light/dark prefs.

const FONT_SIZE: f32 = 14.0;

fn ordered_selection(a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) {
        (a, b)
    } else {
        (b, a)
    }
}

fn selection_text(session: &Session, visible_cols: usize, visible_rows: usize) -> Option<String> {
    // check for selection, return if none
    let (Some(start), Some(end)) = (session.selection_start, session.selection_end) else {
        return None;
    };
    // order the selection (start,end/end,start)
    let (start, end) = ordered_selection(start, end);
    let grid = &session.terminal.grid;

    let max_row = visible_rows.min(grid.rows).saturating_sub(1);
    if start.0 > max_row {
        return None;
    }
    let end_row = end.0.min(max_row);

    let mut out = String::new();
    for row in start.0..=end_row {
        let row_cells = grid.cells.get(row)?;
        if row_cells.is_empty() {
            continue;
        }
        // get the start and end columns for the row
        let raw_row_start = if row == start.0 { start.1 } else { 0 };
        let raw_row_end = if row == end_row {
            end.1
        } else {
            visible_cols.min(grid.cols).saturating_sub(1) // clamp the end column to the visible columns
        };

        let max_col = row_cells.len().saturating_sub(1);
        let row_start = raw_row_start.min(max_col);
        let row_end = raw_row_end.min(max_col);
        if row_start > row_end {
            continue;
        }

        let mut line = String::new();
        for cell in row_cells.iter().take(row_end + 1).skip(row_start) {
            let c = cell.c;
            line.push(if c == '\0' { ' ' } else { c }); // if the cell is empty, add a space
        }
        while line.ends_with(' ') {
            line.pop();
        }
        if !out.is_empty() {
            out.push('\n'); // add a newline between rows in the selection text
        }
        out.push_str(&line); // add the line to the output
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// this function needs to be BROKEN UP into smaller functions for readability and maintainability
pub fn render(ui: &mut egui::Ui, session: &mut Session) {
    let available = ui.available_rect_before_wrap();
    let font_id = egui::FontId::monospace(FONT_SIZE);
    let galley = ui
        .painter()
        .layout_no_wrap("M".to_string(), font_id.clone(), DEFAULT_FG);
    let char_w = galley.size().x;
    let char_h = galley.size().y;

    if char_w <= 0.0 || char_h <= 0.0 {
        return;
    }

    let visible_cols = ((available.width() / char_w).floor() as usize).max(1);
    let visible_rows = ((available.height() / char_h).floor() as usize).max(1);
    let grid = &session.terminal.grid;
    if grid.cols != visible_cols || grid.rows != visible_rows {
        session.terminal.resize(visible_cols, visible_rows);
        if session.connected {
            let _ = session.cmd_tx.try_send(SshCommand::Resize {
                cols: visible_cols as u32,
                rows: visible_rows as u32,
            });
        }
    }
    let (resp, painter) = ui.allocate_painter(
        egui::vec2(visible_cols as f32 * char_w, visible_rows as f32 * char_h),
        egui::Sense::click_and_drag(), // allow for click and drag selection for putty-like copy behaviore
    );
    let rect = resp.rect;

    // handle the click state
    if resp.clicked() {
        // request focus on the terminal
        ui.memory_mut(|m| m.request_focus(resp.id));
    }

    // handle the selection drag start state
    if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
        session.selection_drag_active = true;
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let max_col = visible_cols
                .min(session.terminal.grid.cols)
                .saturating_sub(1);
            let max_row = visible_rows
                .min(session.terminal.grid.rows)
                .saturating_sub(1);
            let col = (((pos.x - rect.left()) / char_w).floor() as usize).min(max_col);
            let row = (((pos.y - rect.top()) / char_h).floor() as usize).min(max_row);
            session.selection_start = Some((row, col));
            session.selection_end = Some((row, col));
        }
    }

    // handle the selection drag active state
    if session.selection_drag_active && ui.input(|i| i.pointer.primary_down()) {
        if let (Some(_), Some(pos)) = (
            session.selection_start,
            ui.input(|i| i.pointer.interact_pos().or_else(|| i.pointer.latest_pos())),
        ) {
            let max_col = visible_cols
                .min(session.terminal.grid.cols)
                .saturating_sub(1);
            let max_row = visible_rows
                .min(session.terminal.grid.rows)
                .saturating_sub(1);
            let col = (((pos.x - rect.left()) / char_w).floor() as usize).min(max_col);
            let row = (((pos.y - rect.top()) / char_h).floor() as usize).min(max_row);
            session.selection_end = Some((row, col));
        }
    }

    // handle the selection drag release state
    if session.selection_drag_active && ui.input(|i| i.pointer.primary_released()) {
        if let (Some(start), Some(end)) = (session.selection_start, session.selection_end) {
            if start != end {
                if let Some(text) = selection_text(session, visible_cols, visible_rows) {
                    ui.ctx().copy_text(text);
                }
            }
        }
        session.selection_drag_active = false;
    }

    // handle the right click paste behavior
    if resp.clicked_by(egui::PointerButton::Secondary) {
        ui.memory_mut(|m| m.request_focus(resp.id));
        if session.connected {
            if let Ok(mut clip) = arboard::Clipboard::new() {
                if let Ok(text) = clip.get_text() {
                    if !text.is_empty() {
                        let _ = session
                            .cmd_tx
                            .try_send(SshCommand::Input(text.into_bytes()));
                    }
                }
            }
        }
    }
    painter.rect_filled(rect, 0.0, DEFAULT_BG);
    let grid = &session.terminal.grid;
    let total_lines = grid.scrollback.len() + grid.rows;
    let view_height = visible_rows.min(grid.rows);
    let start_line = total_lines.saturating_sub(view_height + grid.scroll_offset);
    for row in 0..visible_rows.min(grid.rows) {
        // render the visible rows
        let y = rect.top() + row as f32 * char_h;
        let line_idx = start_line + row;
        let cells = if line_idx < grid.scrollback.len() {
            &grid.scrollback[line_idx]
        } else {
            &grid.cells[line_idx - grid.scrollback.len()]
        };

        let mut col = 0;
        while col < visible_cols.min(grid.cols) {
            let fg = cells[col].fg;
            let bg = cells[col].bg;
            let start_col = col;
            let mut text = String::new();

            while col < visible_cols.min(grid.cols) && cells[col].fg == fg && cells[col].bg == bg {
                let c = cells[col].c;
                text.push(if c == '\0' { ' ' } else { c });
                col += 1;
            }

            let x = rect.left() + start_col as f32 * char_w;
            let span_w = (col - start_col) as f32 * char_w;
            if bg != DEFAULT_BG {
                // if the background color is not the default, render the background
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(span_w, char_h)),
                    0.0,
                    bg,
                );
            }
            painter.text(
                egui::pos2(x, y),
                egui::Align2::LEFT_TOP,
                &text,
                font_id.clone(),
                fg,
            );
        }
    }
    // handle the selection rendering
    if let (Some(start), Some(end)) = (session.selection_start, session.selection_end) {
        //
        let (start, end) = ordered_selection(start, end);
        let max_row = visible_rows.min(grid.rows).saturating_sub(1);
        let max_col = visible_cols.min(grid.cols).saturating_sub(1);
        let end_row = end.0.min(max_row);
        let selection_fill = egui::Color32::from_rgba_premultiplied(120, 160, 220, 80);

        for row in start.0..=end_row {
            let row_start_col = if row == start.0 { start.1 } else { 0 };
            let row_end_col = if row == end_row { end.1 } else { max_col };
            if row_start_col > row_end_col {
                continue;
            }

            let clamped_start = row_start_col.min(max_col);
            let clamped_end = row_end_col.min(max_col);
            let x = rect.left() + clamped_start as f32 * char_w;
            let y = rect.top() + row as f32 * char_h;
            let w = (clamped_end - clamped_start + 1) as f32 * char_w;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, char_h)),
                0.0,
                selection_fill,
            );
        }
    }
    // handle the cursor rendering
    if grid.cursor_visible && grid.cursor_row < visible_rows && grid.cursor_col < visible_cols {
        let cx = rect.left() + grid.cursor_col as f32 * char_w;
        let cy = rect.top() + grid.cursor_row as f32 * char_h;
        let cursor_rect = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(char_w, char_h));
        painter.rect_filled(
            cursor_rect,
            0.0,
            egui::Color32::from_rgba_premultiplied(200, 200, 200, 120),
        );
    }
    // handle the input handling
    if ui.memory(|m| m.has_focus(resp.id)) {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });

        let mut input_bytes: Vec<u8> = Vec::new();
        ui.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Copy => {
                        input_bytes.push(0x03);
                    }
                    egui::Event::Paste(text) => {
                        if !text.is_empty() {
                            input_bytes.extend_from_slice(text.as_bytes());
                        }
                    }
                    egui::Event::Text(text) => {
                        for c in text.chars() {
                            if c >= ' ' && c != '\x7f' {
                                let mut buf = [0u8; 4];
                                let encoded = c.encode_utf8(&mut buf);
                                input_bytes.extend_from_slice(encoded.as_bytes());
                            }
                        }
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if !modifiers.ctrl && !modifiers.command {
                            if *key == egui::Key::PageUp {
                                let max_offset = session.terminal.grid.scrollback.len();
                                session.terminal.grid.scroll_offset =
                                    (session.terminal.grid.scroll_offset + visible_rows)
                                        .min(max_offset);
                                continue;
                            }
                            if *key == egui::Key::PageDown {
                                session.terminal.grid.scroll_offset = session
                                    .terminal
                                    .grid
                                    .scroll_offset
                                    .saturating_sub(visible_rows);
                                continue;
                            }
                        }

                        // ctrl >< and delete key handling
                        if modifiers.ctrl || modifiers.command {
                            if *key == egui::Key::ArrowLeft {
                                input_bytes.extend_from_slice(b"\x1b[1;5D"); // move the cursor left
                                continue;
                            }
                            if *key == egui::Key::ArrowRight {
                                input_bytes.extend_from_slice(b"\x1b[1;5C"); // move the cursor right
                                continue;
                            }
                            if *key == egui::Key::Delete {
                                input_bytes.extend_from_slice(b"\x1bd"); // delete the character under the cursor
                                continue;
                            }
                            if modifiers.shift && *key == egui::Key::C {
                                if let Some(text) =
                                    selection_text(session, visible_cols, visible_rows)
                                {
                                    ui.ctx().copy_text(text);
                                }
                                continue;
                            }

                            let code: Option<u8> = match key {
                                // ctrl a-z and k-t handling
                                egui::Key::A => Some(0x01),
                                egui::Key::B => Some(0x02),
                                egui::Key::C => Some(0x03),
                                egui::Key::D => Some(0x04),
                                egui::Key::E => Some(0x05),
                                egui::Key::F => Some(0x06),
                                egui::Key::K => Some(0x0B),
                                egui::Key::L => Some(0x0C),
                                egui::Key::N => Some(0x0E),
                                egui::Key::P => Some(0x10),
                                egui::Key::R => Some(0x12),
                                egui::Key::T => Some(0x14),
                                egui::Key::U => Some(0x15),
                                egui::Key::W => Some(0x17),
                                egui::Key::Z => Some(0x1A),
                                _ => None,
                            };
                            if let Some(c) = code {
                                input_bytes.push(c);
                            }
                            if modifiers.shift && *key == egui::Key::V {
                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                    if let Ok(text) = clip.get_text() {
                                        input_bytes.extend_from_slice(text.as_bytes());
                                    }
                                }
                            }
                        } else {
                            let seq: Option<&[u8]> = match key {
                                egui::Key::Enter => Some(b"\r"),
                                egui::Key::Backspace => Some(&[0x7f]),
                                egui::Key::Tab => Some(b"\t"),
                                egui::Key::Escape => Some(&[0x1b]),
                                egui::Key::ArrowUp => Some(b"\x1b[A"),
                                egui::Key::ArrowDown => Some(b"\x1b[B"),
                                egui::Key::ArrowRight => Some(b"\x1b[C"),
                                egui::Key::ArrowLeft => Some(b"\x1b[D"),
                                egui::Key::Home => Some(b"\x1b[H"),
                                egui::Key::End => Some(b"\x1b[F"),
                                egui::Key::PageUp => Some(b"\x1b[5~"),
                                egui::Key::PageDown => Some(b"\x1b[6~"),
                                egui::Key::Delete => Some(b"\x1b[3~"),
                                egui::Key::Insert => Some(b"\x1b[2~"),
                                egui::Key::F1 => Some(b"\x1bOP"),
                                egui::Key::F2 => Some(b"\x1bOQ"),
                                egui::Key::F3 => Some(b"\x1bOR"),
                                egui::Key::F4 => Some(b"\x1bOS"),
                                egui::Key::F5 => Some(b"\x1b[15~"),
                                egui::Key::F6 => Some(b"\x1b[17~"),
                                egui::Key::F7 => Some(b"\x1b[18~"),
                                egui::Key::F8 => Some(b"\x1b[19~"),
                                egui::Key::F9 => Some(b"\x1b[20~"),
                                egui::Key::F10 => Some(b"\x1b[21~"),
                                egui::Key::F11 => Some(b"\x1b[23~"),
                                egui::Key::F12 => Some(b"\x1b[24~"),
                                _ => None,
                            };
                            if let Some(s) = seq {
                                input_bytes.extend_from_slice(s);
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        ui.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                input_bytes.push(b'\t');
            }
        });

        if !input_bytes.is_empty() {
            session.selection_start = None;
            session.selection_end = None;
            session.selection_drag_active = false;
        }
        if !input_bytes.is_empty() && session.connected {
            let _ = session.cmd_tx.try_send(SshCommand::Input(input_bytes));
        }
    }
    // handle the scroll behavior
    let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
    if resp.hovered() && scroll_delta != 0.0 {
        let lines = (scroll_delta / char_h).round() as i32; // convert the scroll delta to lines
        let grid = &mut session.terminal.grid;
        let max_offset = grid.scrollback.len(); // get the maximum offset
        let new_offset = (grid.scroll_offset as i32 + lines).clamp(0, max_offset as i32); // clamp the new offset to the maximum offset
        grid.scroll_offset = new_offset as usize; // set the new offset
    }
}
