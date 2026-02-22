use eframe::egui::Color32;
use std::collections::VecDeque;

const MAX_SCROLLBACK: usize = 10_000;

pub const ANSI_COLORS: [Color32; 8] = [
    Color32::from_rgb(0, 0, 0),
    Color32::from_rgb(205, 49, 49),
    Color32::from_rgb(13, 188, 121),
    Color32::from_rgb(229, 229, 16),
    Color32::from_rgb(36, 114, 200),
    Color32::from_rgb(188, 63, 188),
    Color32::from_rgb(17, 168, 205),
    Color32::from_rgb(229, 229, 229),
];

pub const ANSI_BRIGHT: [Color32; 8] = [
    Color32::from_rgb(102, 102, 102),
    Color32::from_rgb(241, 76, 76),
    Color32::from_rgb(35, 209, 139),
    Color32::from_rgb(245, 245, 67),
    Color32::from_rgb(59, 142, 234),
    Color32::from_rgb(214, 112, 214),
    Color32::from_rgb(41, 184, 219),
    Color32::from_rgb(255, 255, 255),
];

pub const DEFAULT_FG: Color32 = Color32::from_rgb(204, 204, 204);
pub const DEFAULT_BG: Color32 = Color32::from_rgb(30, 30, 30);

fn color_from_256(idx: u16) -> Color32 {
    match idx {
        0..=7 => ANSI_COLORS[idx as usize],
        8..=15 => ANSI_BRIGHT[(idx - 8) as usize],
        16..=231 => {
            let i = idx - 16;
            let r = (i / 36) as u8;
            let g = ((i % 36) / 6) as u8;
            let b = (i % 6) as u8;
            let f = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            Color32::from_rgb(f(r), f(g), f(b))
        }
        232..=255 => {
            let v = (8 + 10 * (idx - 232)) as u8;
            Color32::from_rgb(v, v, v)
        }
        _ => DEFAULT_FG,
    }
}

#[derive(Clone, Copy)]
pub struct Cell {
    pub c: char,
    pub fg: Color32,
    pub bg: Color32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

pub struct TerminalGrid {
    pub cells: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub rows: usize,
    pub cols: usize,
    pub scrollback: VecDeque<Vec<Cell>>,
    pub scroll_offset: usize,
    cur_fg: Color32,
    cur_bg: Color32,
    cur_bold: bool,
    cur_italic: bool,
    cur_underline: bool,
    scroll_top: usize,
    scroll_bottom: usize,
    pub cursor_visible: bool,
    alt_screen: bool,
    saved_cells: Option<Vec<Vec<Cell>>>,
    saved_cursor: (usize, usize),
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cells,
            cursor_row: 0,
            cursor_col: 0,
            rows,
            cols,
            scrollback: VecDeque::new(),
            scroll_offset: 0,
            cur_fg: DEFAULT_FG,
            cur_bg: DEFAULT_BG,
            cur_bold: false,
            cur_italic: false,
            cur_underline: false,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            cursor_visible: true,
            alt_screen: false,
            saved_cells: None,
            saved_cursor: (0, 0),
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }
        let mut new_cells = vec![vec![Cell::default(); new_cols]; new_rows];
        let copy_rows = new_rows.min(self.rows);
        let copy_cols = new_cols.min(self.cols);
        for (r, new_row) in new_cells.iter_mut().enumerate().take(copy_rows) {
            for (c, new_cell) in new_row.iter_mut().enumerate().take(copy_cols) {
                *new_cell = self.cells[r][c];
            }
        }
        self.cells = new_cells;
        self.rows = new_rows;
        self.cols = new_cols;
        self.scroll_bottom = new_rows.saturating_sub(1);
        self.cursor_row = self.cursor_row.min(new_rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
    }

    fn current_cell(&self) -> Cell {
        Cell {
            c: ' ',
            fg: self.cur_fg,
            bg: self.cur_bg,
            bold: self.cur_bold,
            italic: self.cur_italic,
            underline: self.cur_underline,
        }
    }

    fn scroll_up(&mut self) {
        if self.scroll_top == 0 && !self.alt_screen {
            let row = self.cells[0].clone();
            self.scrollback.push_back(row);
            if self.scrollback.len() > MAX_SCROLLBACK {
                self.scrollback.pop_front();
            }
        }
        for r in self.scroll_top..self.scroll_bottom {
            self.cells[r] = self.cells[r + 1].clone();
        }
        self.cells[self.scroll_bottom] = vec![Cell::default(); self.cols];
    }

    fn scroll_down(&mut self) {
        for r in (self.scroll_top + 1..=self.scroll_bottom).rev() {
            self.cells[r] = self.cells[r - 1].clone();
        }
        self.cells[self.scroll_top] = vec![Cell::default(); self.cols];
    }

    fn newline(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up();
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
    }

    fn reset_attributes(&mut self) {
        self.cur_fg = DEFAULT_FG;
        self.cur_bg = DEFAULT_BG;
        self.cur_bold = false;
        self.cur_italic = false;
        self.cur_underline = false;
    }

    fn handle_sgr(&mut self, params: &vte::Params) {
        if params.is_empty() {
            self.reset_attributes();
            return;
        }
        let mut iter = params.iter();
        while let Some(sub) = iter.next() {
            let code = sub.first().copied().unwrap_or(0);
            match code {
                0 => self.reset_attributes(),
                1 => self.cur_bold = true,
                2 => self.cur_bold = false,
                3 => self.cur_italic = true,
                4 => self.cur_underline = true,
                7 => {
                    std::mem::swap(&mut self.cur_fg, &mut self.cur_bg);
                }
                22 => self.cur_bold = false,
                23 => self.cur_italic = false,
                24 => self.cur_underline = false,
                27 => {}
                30..=37 => self.cur_fg = ANSI_COLORS[(code - 30) as usize],
                38 => {
                    if let Some(mode) = iter.next() {
                        match mode.first().copied().unwrap_or(0) {
                            5 => {
                                if let Some(idx) = iter.next() {
                                    self.cur_fg = color_from_256(idx.first().copied().unwrap_or(0));
                                }
                            }
                            2 => {
                                let r =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                let g =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                let b =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                self.cur_fg = Color32::from_rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.cur_fg = DEFAULT_FG,
                40..=47 => self.cur_bg = ANSI_COLORS[(code - 40) as usize],
                48 => {
                    if let Some(mode) = iter.next() {
                        match mode.first().copied().unwrap_or(0) {
                            5 => {
                                if let Some(idx) = iter.next() {
                                    self.cur_bg = color_from_256(idx.first().copied().unwrap_or(0));
                                }
                            }
                            2 => {
                                let r =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                let g =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                let b =
                                    iter.next().and_then(|s| s.first().copied()).unwrap_or(0) as u8;
                                self.cur_bg = Color32::from_rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.cur_bg = DEFAULT_BG,
                90..=97 => self.cur_fg = ANSI_BRIGHT[(code - 90) as usize],
                100..=107 => self.cur_bg = ANSI_BRIGHT[(code - 100) as usize],
                _ => {}
            }
        }
    }
}

impl vte::Perform for TerminalGrid {
    fn print(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.newline();
        }
        let mut cell = self.current_cell();
        cell.c = c;
        self.cells[self.cursor_row][self.cursor_col] = cell;
        self.cursor_col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0B | 0x0C => self.newline(),
            b'\r' => self.cursor_col = 0,
            0x08 => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            b'\t' => {
                let next = (self.cursor_col + 8) & !7;
                self.cursor_col = next.min(self.cols - 1);
            }
            0x07 => {}
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let p = |idx: usize, default: u16| -> u16 {
            params
                .iter()
                .nth(idx)
                .and_then(|s| s.first().copied())
                .filter(|&v| v != 0)
                .unwrap_or(default)
        };

        let is_private = intermediates.first() == Some(&b'?');

        match action {
            'A' => {
                let n = p(0, 1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            'B' => {
                let n = p(0, 1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
            }
            'C' => {
                let n = p(0, 1) as usize;
                self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
            }
            'D' => {
                let n = p(0, 1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            'E' => {
                let n = p(0, 1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                self.cursor_col = 0;
            }
            'F' => {
                let n = p(0, 1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            'G' => {
                let col = p(0, 1) as usize;
                self.cursor_col = (col - 1).min(self.cols - 1);
            }
            'H' | 'f' => {
                let row = p(0, 1) as usize;
                let col = p(1, 1) as usize;
                self.cursor_row = (row - 1).min(self.rows - 1);
                self.cursor_col = (col - 1).min(self.cols - 1);
            }
            'J' => {
                let mode = p(0, 0);
                let blank = Cell::default();
                match mode {
                    0 => {
                        for c in self.cursor_col..self.cols {
                            self.cells[self.cursor_row][c] = blank;
                        }
                        for r in (self.cursor_row + 1)..self.rows {
                            self.cells[r] = vec![blank; self.cols];
                        }
                    }
                    1 => {
                        for r in 0..self.cursor_row {
                            self.cells[r] = vec![blank; self.cols];
                        }
                        for c in 0..=self.cursor_col {
                            self.cells[self.cursor_row][c] = blank;
                        }
                    }
                    2 | 3 => {
                        for r in 0..self.rows {
                            self.cells[r] = vec![blank; self.cols];
                        }
                    }
                    _ => {}
                }
            }
            'K' => {
                let mode = p(0, 0);
                let blank = Cell::default();
                match mode {
                    0 => {
                        for c in self.cursor_col..self.cols {
                            self.cells[self.cursor_row][c] = blank;
                        }
                    }
                    1 => {
                        for c in 0..=self.cursor_col {
                            self.cells[self.cursor_row][c] = blank;
                        }
                    }
                    2 => {
                        self.cells[self.cursor_row] = vec![blank; self.cols];
                    }
                    _ => {}
                }
            }
            'L' => {
                let n = p(0, 1) as usize;
                for _ in 0..n {
                    if self.cursor_row <= self.scroll_bottom {
                        self.cells.remove(self.scroll_bottom);
                        self.cells
                            .insert(self.cursor_row, vec![Cell::default(); self.cols]);
                    }
                }
            }
            'M' => {
                let n = p(0, 1) as usize;
                for _ in 0..n {
                    if self.cursor_row <= self.scroll_bottom {
                        self.cells.remove(self.cursor_row);
                        self.cells
                            .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
                    }
                }
            }
            'P' => {
                let n = (p(0, 1) as usize).min(self.cols - self.cursor_col);
                let row = &mut self.cells[self.cursor_row];
                for _ in 0..n {
                    if self.cursor_col < row.len() {
                        row.remove(self.cursor_col);
                        row.push(Cell::default());
                    }
                }
            }
            '@' => {
                let n = (p(0, 1) as usize).min(self.cols - self.cursor_col);
                let row = &mut self.cells[self.cursor_row];
                for _ in 0..n {
                    row.insert(self.cursor_col, Cell::default());
                    row.truncate(self.cols);
                }
            }
            'X' => {
                let n = (p(0, 1) as usize).min(self.cols - self.cursor_col);
                for i in 0..n {
                    self.cells[self.cursor_row][self.cursor_col + i] = Cell::default();
                }
            }
            'd' => {
                let row = p(0, 1) as usize;
                self.cursor_row = (row - 1).min(self.rows - 1);
            }
            'm' => self.handle_sgr(params),
            'r' => {
                let top = p(0, 1) as usize;
                let bottom = p(1, self.rows as u16) as usize;
                self.scroll_top = (top - 1).min(self.rows - 1);
                self.scroll_bottom = (bottom - 1).min(self.rows - 1);
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            'h' => {
                if is_private {
                    let mode = p(0, 0);
                    match mode {
                        25 => self.cursor_visible = true,
                        1049 => {
                            self.saved_cells = Some(self.cells.clone());
                            self.saved_cursor = (self.cursor_row, self.cursor_col);
                            self.cells = vec![vec![Cell::default(); self.cols]; self.rows];
                            self.cursor_row = 0;
                            self.cursor_col = 0;
                            self.alt_screen = true;
                        }
                        _ => {}
                    }
                }
            }
            'l' => {
                if is_private {
                    let mode = p(0, 0);
                    match mode {
                        25 => self.cursor_visible = false,
                        1049 => {
                            if let Some(saved) = self.saved_cells.take() {
                                self.cells = saved;
                                self.cursor_row = self.saved_cursor.0;
                                self.cursor_col = self.saved_cursor.1;
                            }
                            self.alt_screen = false;
                        }
                        _ => {}
                    }
                }
            }
            'S' => {
                let n = p(0, 1) as usize;
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            'T' => {
                let n = p(0, 1) as usize;
                for _ in 0..n {
                    self.scroll_down();
                }
            }
            _ => {
                log::trace!("Unhandled CSI: {:?} {} {:?}", params, action, intermediates);
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            (_, b'M') => {
                if self.cursor_row == self.scroll_top {
                    self.scroll_down();
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            (_, b'D') => {
                self.newline();
            }
            (_, b'E') => {
                self.cursor_col = 0;
                self.newline();
            }
            (_, b'7') => {
                self.saved_cursor = (self.cursor_row, self.cursor_col);
            }
            (_, b'8') => {
                self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
            }
            _ => {
                log::trace!("Unhandled ESC: {:?} 0x{:02x}", intermediates, byte);
            }
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
}
pub struct Terminal {
    pub grid: TerminalGrid,
    parser: vte::Parser,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: TerminalGrid::new(cols, rows),
            parser: vte::Parser::new(),
        }
    }
    pub fn process(&mut self, data: &[u8]) {
        let Terminal {
            ref mut grid,
            ref mut parser,
        } = *self;
        parser.advance(grid, data);
        grid.scroll_offset = 0;
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.grid.resize(cols, rows);
    }
}
