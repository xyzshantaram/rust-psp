//! Debug support.
//!
//! You should use the `dprintln!` and `dprint!` macros.

use crate::sys;
use core::fmt;

#[macro_export]
macro_rules! dprintln {
    ($($arg:tt)*) => {
        $crate::debug::print_args(core::format_args!($($arg)*));
        $crate::debug::print_args(core::format_args!("\n"));
    }
}

#[macro_export]
macro_rules! dprint {
    ($($arg:tt)*) => {
        $crate::debug::print_args(core::format_args!($($arg)*))
    }
}

// TODO: Wrap this in some kind of a mutex.
static mut CHARS: CharBuffer = CharBuffer::new();

/// Update the screen.
fn update() {
    unsafe {
        init();
        clear_screen(0);

        for (i, line) in CHARS.lines().enumerate() {
            put_str::<MsxFont>(
                &line.chars[0..line.len],
                0,
                i * MsxFont::CHAR_HEIGHT,
                0xffff_ffff,
            )
        }
    }
}

trait Font {
    const CHAR_WIDTH: usize;
    const CHAR_HEIGHT: usize;

    fn put_char(x: usize, y: usize, color: u32, c: u8);
}

struct MsxFont;

impl Font for MsxFont {
    const CHAR_HEIGHT: usize = 10;
    const CHAR_WIDTH: usize = 6;

    fn put_char(x: usize, y: usize, color: u32, c: u8) {
        unsafe {
            let mut ptr = VRAM_BASE
                .offset(x as isize)
                .offset((y * BUFFER_WIDTH) as isize);

            for i in 0..8 {
                for j in 0..8 {
                    if MSX_FONT[c as usize * 8 + i] & (0b1000_0000 >> j) != 0 {
                        *ptr = color;
                    }

                    ptr = ptr.offset(1);
                }

                ptr = ptr.offset(-8).offset(BUFFER_WIDTH as isize);
            }
        }
    }
}

const BUFFER_WIDTH: usize = 512;
const DISPLAY_HEIGHT: usize = 272;
const DISPLAY_WIDTH: usize = 480;
static mut VRAM_BASE: *mut u32 = 0 as *mut u32;

unsafe fn clear_screen(color: u32) {
    let mut ptr = VRAM_BASE;

    for _ in 0..(BUFFER_WIDTH * DISPLAY_HEIGHT) {
        *ptr = color;
        ptr = ptr.offset(1);
    }
}

unsafe fn put_str<T: Font>(s: &[u8], x: usize, y: usize, color: u32) {
    if y > DISPLAY_HEIGHT {
        return;
    }

    for (i, c) in s.iter().enumerate() {
        if i >= (DISPLAY_WIDTH / T::CHAR_WIDTH) as usize {
            break;
        }

        if *c as u32 <= 255 && *c != b'\0' {
            T::put_char(T::CHAR_WIDTH * i + x, y, color, *c);
        }
    }
}

unsafe fn init() {
    // The OR operation here specifies the address bypasses cache.
    VRAM_BASE = (0x4000_0000u32 | sys::ge::sce_ge_edram_get_addr() as u32) as *mut u32;

    // TODO: Change sys types to usize.
    sys::display::sce_display_set_mode(sys::display::DisplayMode::Lcd, DISPLAY_WIDTH, DISPLAY_HEIGHT);
    sys::display::sce_display_set_frame_buf(
        VRAM_BASE as *const u8,
        BUFFER_WIDTH,
        sys::display::DisplayPixelFormat::Psm8888,
        sys::display::DisplaySetBufSync::NextFrame,
    );
}

#[doc(hidden)]
pub fn print_args(arguments: core::fmt::Arguments<'_>) {
    use fmt::Write;

    unsafe {
        let _ = write!(CHARS, "{}", arguments);
    }

    update();
}

// TODO: Move to font.
const ROWS: usize = DISPLAY_HEIGHT / MsxFont::CHAR_HEIGHT;
const COLS: usize = DISPLAY_WIDTH / MsxFont::CHAR_WIDTH;

#[derive(Copy, Clone)]
struct Line {
    chars: [u8; COLS],
    len: usize,
}

impl Line {
    const fn new() -> Self {
        Self {
            chars: [0; COLS],
            len: 0,
        }
    }
}

struct CharBuffer {
    lines: [Line; ROWS],
    written: usize,
    advance_next: bool,
}

impl CharBuffer {
    const fn new() -> Self {
        Self {
            lines: [Line::new(); ROWS],
            written: 0,
            advance_next: false,
        }
    }

    fn advance(&mut self) {
        self.written += 1;
        if self.written >= ROWS {
            *self.current_line() = Line::new();
        }
    }

    fn current_line(&mut self) -> &mut Line {
        &mut self.lines[self.written % ROWS]
    }

    fn add(&mut self, c: u8) {
        if self.advance_next {
            self.advance_next = false;
            self.advance();
        }

        match c {
            b'\n' => self.advance_next = true,
            b'\t' => {
                self.add(b' ');
                self.add(b' ');
                self.add(b' ');
                self.add(b' ');
            }

            _ => {
                if self.current_line().len == COLS  {
                    self.advance();
                }

                let line = self.current_line();
                line.chars[line.len] = c;
                line.len += 1;
            }
        }
    }

    fn lines(&self) -> LineIter<'_> {
        LineIter {
            buf: self,
            pos: 0,
        }
    }
}

impl fmt::Write for CharBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            for c in s.chars() {
                match c as u32 {
                    0..=255 => CHARS.add(c as u8),
                    _ => CHARS.add(0),
                }
            }
        }

        Ok(())
    }
}

struct LineIter<'a> {
    buf: &'a CharBuffer,
    pos: usize,
}

impl<'a> Iterator for LineIter<'a> {
    type Item = Line;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < core::cmp::min(self.buf.written + 1, ROWS) {
            let idx = if self.buf.written > ROWS {
                (self.buf.written + 1 + self.pos) % ROWS
            } else {
                self.pos
            };

            let line = self.buf.lines[idx];
            self.pos += 1;
            Some(line)
        } else {
            None
        }
    }
}

/// Raw MSX font.
///
/// This is an 8bit x 256 black and white image.
const MSX_FONT: [u8; 2048] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x42, 0xa5, 0x81,
    0xa5, 0x99, 0x42, 0x3c, 0x3c, 0x7e, 0xdb, 0xff, 0xff, 0xdb, 0x66, 0x3c,
    0x6c, 0xfe, 0xfe, 0xfe, 0x7c, 0x38, 0x10, 0x00, 0x10, 0x38, 0x7c, 0xfe,
    0x7c, 0x38, 0x10, 0x00, 0x10, 0x38, 0x54, 0xfe, 0x54, 0x10, 0x38, 0x00,
    0x10, 0x38, 0x7c, 0xfe, 0xfe, 0x10, 0x38, 0x00, 0x00, 0x00, 0x00, 0x30,
    0x30, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xe7, 0xe7, 0xff, 0xff, 0xff,
    0x38, 0x44, 0x82, 0x82, 0x82, 0x44, 0x38, 0x00, 0xc7, 0xbb, 0x7d, 0x7d,
    0x7d, 0xbb, 0xc7, 0xff, 0x0f, 0x03, 0x05, 0x79, 0x88, 0x88, 0x88, 0x70,
    0x38, 0x44, 0x44, 0x44, 0x38, 0x10, 0x7c, 0x10, 0x30, 0x28, 0x24, 0x24,
    0x28, 0x20, 0xe0, 0xc0, 0x3c, 0x24, 0x3c, 0x24, 0x24, 0xe4, 0xdc, 0x18,
    0x10, 0x54, 0x38, 0xee, 0x38, 0x54, 0x10, 0x00, 0x10, 0x10, 0x10, 0x7c,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0xff, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xff, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0xf0,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0xff, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x1f, 0x10, 0x10, 0x10, 0x10, 0x00, 0x00, 0x00, 0xf0,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f, 0x00, 0x00, 0x00, 0x00,
    0x10, 0x10, 0x10, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x81, 0x42, 0x24, 0x18,
    0x18, 0x24, 0x42, 0x81, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80,
    0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01, 0x00, 0x10, 0x10, 0xff,
    0x10, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x20, 0x20, 0x20, 0x20, 0x00, 0x00, 0x20, 0x00, 0x50, 0x50, 0x50, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x50, 0x50, 0xf8, 0x50, 0xf8, 0x50, 0x50, 0x00,
    0x20, 0x78, 0xa0, 0x70, 0x28, 0xf0, 0x20, 0x00, 0xc0, 0xc8, 0x10, 0x20,
    0x40, 0x98, 0x18, 0x00, 0x40, 0xa0, 0x40, 0xa8, 0x90, 0x98, 0x60, 0x00,
    0x10, 0x20, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x20, 0x40, 0x40,
    0x40, 0x20, 0x10, 0x00, 0x40, 0x20, 0x10, 0x10, 0x10, 0x20, 0x40, 0x00,
    0x20, 0xa8, 0x70, 0x20, 0x70, 0xa8, 0x20, 0x00, 0x00, 0x20, 0x20, 0xf8,
    0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x20, 0x40,
    0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x60, 0x60, 0x00, 0x00, 0x00, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00,
    0x70, 0x88, 0x98, 0xa8, 0xc8, 0x88, 0x70, 0x00, 0x20, 0x60, 0xa0, 0x20,
    0x20, 0x20, 0xf8, 0x00, 0x70, 0x88, 0x08, 0x10, 0x60, 0x80, 0xf8, 0x00,
    0x70, 0x88, 0x08, 0x30, 0x08, 0x88, 0x70, 0x00, 0x10, 0x30, 0x50, 0x90,
    0xf8, 0x10, 0x10, 0x00, 0xf8, 0x80, 0xe0, 0x10, 0x08, 0x10, 0xe0, 0x00,
    0x30, 0x40, 0x80, 0xf0, 0x88, 0x88, 0x70, 0x00, 0xf8, 0x88, 0x10, 0x20,
    0x20, 0x20, 0x20, 0x00, 0x70, 0x88, 0x88, 0x70, 0x88, 0x88, 0x70, 0x00,
    0x70, 0x88, 0x88, 0x78, 0x08, 0x10, 0x60, 0x00, 0x00, 0x00, 0x20, 0x00,
    0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x20, 0x20, 0x40,
    0x18, 0x30, 0x60, 0xc0, 0x60, 0x30, 0x18, 0x00, 0x00, 0x00, 0xf8, 0x00,
    0xf8, 0x00, 0x00, 0x00, 0xc0, 0x60, 0x30, 0x18, 0x30, 0x60, 0xc0, 0x00,
    0x70, 0x88, 0x08, 0x10, 0x20, 0x00, 0x20, 0x00, 0x70, 0x88, 0x08, 0x68,
    0xa8, 0xa8, 0x70, 0x00, 0x20, 0x50, 0x88, 0x88, 0xf8, 0x88, 0x88, 0x00,
    0xf0, 0x48, 0x48, 0x70, 0x48, 0x48, 0xf0, 0x00, 0x30, 0x48, 0x80, 0x80,
    0x80, 0x48, 0x30, 0x00, 0xe0, 0x50, 0x48, 0x48, 0x48, 0x50, 0xe0, 0x00,
    0xf8, 0x80, 0x80, 0xf0, 0x80, 0x80, 0xf8, 0x00, 0xf8, 0x80, 0x80, 0xf0,
    0x80, 0x80, 0x80, 0x00, 0x70, 0x88, 0x80, 0xb8, 0x88, 0x88, 0x70, 0x00,
    0x88, 0x88, 0x88, 0xf8, 0x88, 0x88, 0x88, 0x00, 0x70, 0x20, 0x20, 0x20,
    0x20, 0x20, 0x70, 0x00, 0x38, 0x10, 0x10, 0x10, 0x90, 0x90, 0x60, 0x00,
    0x88, 0x90, 0xa0, 0xc0, 0xa0, 0x90, 0x88, 0x00, 0x80, 0x80, 0x80, 0x80,
    0x80, 0x80, 0xf8, 0x00, 0x88, 0xd8, 0xa8, 0xa8, 0x88, 0x88, 0x88, 0x00,
    0x88, 0xc8, 0xc8, 0xa8, 0x98, 0x98, 0x88, 0x00, 0x70, 0x88, 0x88, 0x88,
    0x88, 0x88, 0x70, 0x00, 0xf0, 0x88, 0x88, 0xf0, 0x80, 0x80, 0x80, 0x00,
    0x70, 0x88, 0x88, 0x88, 0xa8, 0x90, 0x68, 0x00, 0xf0, 0x88, 0x88, 0xf0,
    0xa0, 0x90, 0x88, 0x00, 0x70, 0x88, 0x80, 0x70, 0x08, 0x88, 0x70, 0x00,
    0xf8, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x00, 0x88, 0x88, 0x88, 0x88,
    0x88, 0x88, 0x70, 0x00, 0x88, 0x88, 0x88, 0x88, 0x50, 0x50, 0x20, 0x00,
    0x88, 0x88, 0x88, 0xa8, 0xa8, 0xd8, 0x88, 0x00, 0x88, 0x88, 0x50, 0x20,
    0x50, 0x88, 0x88, 0x00, 0x88, 0x88, 0x88, 0x70, 0x20, 0x20, 0x20, 0x00,
    0xf8, 0x08, 0x10, 0x20, 0x40, 0x80, 0xf8, 0x00, 0x70, 0x40, 0x40, 0x40,
    0x40, 0x40, 0x70, 0x00, 0x00, 0x00, 0x80, 0x40, 0x20, 0x10, 0x08, 0x00,
    0x70, 0x10, 0x10, 0x10, 0x10, 0x10, 0x70, 0x00, 0x20, 0x50, 0x88, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x00,
    0x40, 0x20, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x08,
    0x78, 0x88, 0x78, 0x00, 0x80, 0x80, 0xb0, 0xc8, 0x88, 0xc8, 0xb0, 0x00,
    0x00, 0x00, 0x70, 0x88, 0x80, 0x88, 0x70, 0x00, 0x08, 0x08, 0x68, 0x98,
    0x88, 0x98, 0x68, 0x00, 0x00, 0x00, 0x70, 0x88, 0xf8, 0x80, 0x70, 0x00,
    0x10, 0x28, 0x20, 0xf8, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0x68, 0x98,
    0x98, 0x68, 0x08, 0x70, 0x80, 0x80, 0xf0, 0x88, 0x88, 0x88, 0x88, 0x00,
    0x20, 0x00, 0x60, 0x20, 0x20, 0x20, 0x70, 0x00, 0x10, 0x00, 0x30, 0x10,
    0x10, 0x10, 0x90, 0x60, 0x40, 0x40, 0x48, 0x50, 0x60, 0x50, 0x48, 0x00,
    0x60, 0x20, 0x20, 0x20, 0x20, 0x20, 0x70, 0x00, 0x00, 0x00, 0xd0, 0xa8,
    0xa8, 0xa8, 0xa8, 0x00, 0x00, 0x00, 0xb0, 0xc8, 0x88, 0x88, 0x88, 0x00,
    0x00, 0x00, 0x70, 0x88, 0x88, 0x88, 0x70, 0x00, 0x00, 0x00, 0xb0, 0xc8,
    0xc8, 0xb0, 0x80, 0x80, 0x00, 0x00, 0x68, 0x98, 0x98, 0x68, 0x08, 0x08,
    0x00, 0x00, 0xb0, 0xc8, 0x80, 0x80, 0x80, 0x00, 0x00, 0x00, 0x78, 0x80,
    0xf0, 0x08, 0xf0, 0x00, 0x40, 0x40, 0xf0, 0x40, 0x40, 0x48, 0x30, 0x00,
    0x00, 0x00, 0x90, 0x90, 0x90, 0x90, 0x68, 0x00, 0x00, 0x00, 0x88, 0x88,
    0x88, 0x50, 0x20, 0x00, 0x00, 0x00, 0x88, 0xa8, 0xa8, 0xa8, 0x50, 0x00,
    0x00, 0x00, 0x88, 0x50, 0x20, 0x50, 0x88, 0x00, 0x00, 0x00, 0x88, 0x88,
    0x98, 0x68, 0x08, 0x70, 0x00, 0x00, 0xf8, 0x10, 0x20, 0x40, 0xf8, 0x00,
    0x18, 0x20, 0x20, 0x40, 0x20, 0x20, 0x18, 0x00, 0x20, 0x20, 0x20, 0x00,
    0x20, 0x20, 0x20, 0x00, 0xc0, 0x20, 0x20, 0x10, 0x20, 0x20, 0xc0, 0x00,
    0x40, 0xa8, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x50,
    0xf8, 0x00, 0x00, 0x00, 0x70, 0x88, 0x80, 0x80, 0x88, 0x70, 0x20, 0x60,
    0x90, 0x00, 0x00, 0x90, 0x90, 0x90, 0x68, 0x00, 0x10, 0x20, 0x70, 0x88,
    0xf8, 0x80, 0x70, 0x00, 0x20, 0x50, 0x70, 0x08, 0x78, 0x88, 0x78, 0x00,
    0x48, 0x00, 0x70, 0x08, 0x78, 0x88, 0x78, 0x00, 0x20, 0x10, 0x70, 0x08,
    0x78, 0x88, 0x78, 0x00, 0x20, 0x00, 0x70, 0x08, 0x78, 0x88, 0x78, 0x00,
    0x00, 0x70, 0x80, 0x80, 0x80, 0x70, 0x10, 0x60, 0x20, 0x50, 0x70, 0x88,
    0xf8, 0x80, 0x70, 0x00, 0x50, 0x00, 0x70, 0x88, 0xf8, 0x80, 0x70, 0x00,
    0x20, 0x10, 0x70, 0x88, 0xf8, 0x80, 0x70, 0x00, 0x50, 0x00, 0x00, 0x60,
    0x20, 0x20, 0x70, 0x00, 0x20, 0x50, 0x00, 0x60, 0x20, 0x20, 0x70, 0x00,
    0x40, 0x20, 0x00, 0x60, 0x20, 0x20, 0x70, 0x00, 0x50, 0x00, 0x20, 0x50,
    0x88, 0xf8, 0x88, 0x00, 0x20, 0x00, 0x20, 0x50, 0x88, 0xf8, 0x88, 0x00,
    0x10, 0x20, 0xf8, 0x80, 0xf0, 0x80, 0xf8, 0x00, 0x00, 0x00, 0x6c, 0x12,
    0x7e, 0x90, 0x6e, 0x00, 0x3e, 0x50, 0x90, 0x9c, 0xf0, 0x90, 0x9e, 0x00,
    0x60, 0x90, 0x00, 0x60, 0x90, 0x90, 0x60, 0x00, 0x90, 0x00, 0x00, 0x60,
    0x90, 0x90, 0x60, 0x00, 0x40, 0x20, 0x00, 0x60, 0x90, 0x90, 0x60, 0x00,
    0x40, 0xa0, 0x00, 0xa0, 0xa0, 0xa0, 0x50, 0x00, 0x40, 0x20, 0x00, 0xa0,
    0xa0, 0xa0, 0x50, 0x00, 0x90, 0x00, 0x90, 0x90, 0xb0, 0x50, 0x10, 0xe0,
    0x50, 0x00, 0x70, 0x88, 0x88, 0x88, 0x70, 0x00, 0x50, 0x00, 0x88, 0x88,
    0x88, 0x88, 0x70, 0x00, 0x20, 0x20, 0x78, 0x80, 0x80, 0x78, 0x20, 0x20,
    0x18, 0x24, 0x20, 0xf8, 0x20, 0xe2, 0x5c, 0x00, 0x88, 0x50, 0x20, 0xf8,
    0x20, 0xf8, 0x20, 0x00, 0xc0, 0xa0, 0xa0, 0xc8, 0x9c, 0x88, 0x88, 0x8c,
    0x18, 0x20, 0x20, 0xf8, 0x20, 0x20, 0x20, 0x40, 0x10, 0x20, 0x70, 0x08,
    0x78, 0x88, 0x78, 0x00, 0x10, 0x20, 0x00, 0x60, 0x20, 0x20, 0x70, 0x00,
    0x20, 0x40, 0x00, 0x60, 0x90, 0x90, 0x60, 0x00, 0x20, 0x40, 0x00, 0x90,
    0x90, 0x90, 0x68, 0x00, 0x50, 0xa0, 0x00, 0xa0, 0xd0, 0x90, 0x90, 0x00,
    0x28, 0x50, 0x00, 0xc8, 0xa8, 0x98, 0x88, 0x00, 0x00, 0x70, 0x08, 0x78,
    0x88, 0x78, 0x00, 0xf8, 0x00, 0x60, 0x90, 0x90, 0x90, 0x60, 0x00, 0xf0,
    0x20, 0x00, 0x20, 0x40, 0x80, 0x88, 0x70, 0x00, 0x00, 0x00, 0x00, 0xf8,
    0x80, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x08, 0x08, 0x00, 0x00,
    0x84, 0x88, 0x90, 0xa8, 0x54, 0x84, 0x08, 0x1c, 0x84, 0x88, 0x90, 0xa8,
    0x58, 0xa8, 0x3c, 0x08, 0x20, 0x00, 0x00, 0x20, 0x20, 0x20, 0x20, 0x00,
    0x00, 0x00, 0x24, 0x48, 0x90, 0x48, 0x24, 0x00, 0x00, 0x00, 0x90, 0x48,
    0x24, 0x48, 0x90, 0x00, 0x28, 0x50, 0x20, 0x50, 0x88, 0xf8, 0x88, 0x00,
    0x28, 0x50, 0x70, 0x08, 0x78, 0x88, 0x78, 0x00, 0x28, 0x50, 0x00, 0x70,
    0x20, 0x20, 0x70, 0x00, 0x28, 0x50, 0x00, 0x20, 0x20, 0x20, 0x70, 0x00,
    0x28, 0x50, 0x00, 0x70, 0x88, 0x88, 0x70, 0x00, 0x50, 0xa0, 0x00, 0x60,
    0x90, 0x90, 0x60, 0x00, 0x28, 0x50, 0x00, 0x88, 0x88, 0x88, 0x70, 0x00,
    0x50, 0xa0, 0x00, 0xa0, 0xa0, 0xa0, 0x50, 0x00, 0xfc, 0x48, 0x48, 0x48,
    0xe8, 0x08, 0x50, 0x20, 0x00, 0x50, 0x00, 0x50, 0x50, 0x50, 0x10, 0x20,
    0xc0, 0x44, 0xc8, 0x54, 0xec, 0x54, 0x9e, 0x04, 0x10, 0xa8, 0x40, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x50, 0x88, 0x50, 0x20, 0x00, 0x00,
    0x88, 0x10, 0x20, 0x40, 0x80, 0x28, 0x00, 0x00, 0x7c, 0xa8, 0xa8, 0x68,
    0x28, 0x28, 0x28, 0x00, 0x38, 0x40, 0x30, 0x48, 0x48, 0x30, 0x08, 0x70,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xf0, 0xf0, 0xf0, 0xf0,
    0x0f, 0x0f, 0x0f, 0x0f, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c,
    0x3c, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00,
    0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0x0f, 0x0f, 0x0f, 0x0f,
    0xf0, 0xf0, 0xf0, 0xf0, 0xfc, 0xfc, 0xfc, 0xfc, 0xfc, 0xfc, 0xfc, 0xfc,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x3f, 0x3f, 0x3f, 0x3f,
    0x3f, 0x3f, 0x3f, 0x3f, 0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88,
    0x88, 0x44, 0x22, 0x11, 0x88, 0x44, 0x22, 0x11, 0xfe, 0x7c, 0x38, 0x10,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x38, 0x7c, 0xfe,
    0x80, 0xc0, 0xe0, 0xf0, 0xe0, 0xc0, 0x80, 0x00, 0x01, 0x03, 0x07, 0x0f,
    0x07, 0x03, 0x01, 0x00, 0xff, 0x7e, 0x3c, 0x18, 0x18, 0x3c, 0x7e, 0xff,
    0x81, 0xc3, 0xe7, 0xff, 0xff, 0xe7, 0xc3, 0x81, 0xf0, 0xf0, 0xf0, 0xf0,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x0f, 0x0f, 0x0f,
    0x0f, 0x0f, 0x0f, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xf0, 0xf0, 0xf0, 0xf0, 0x33, 0x33, 0xcc, 0xcc, 0x33, 0x33, 0xcc, 0xcc,
    0x00, 0x20, 0x20, 0x50, 0x50, 0x88, 0xf8, 0x00, 0x20, 0x20, 0x70, 0x20,
    0x70, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x50, 0x88, 0xa8, 0x50, 0x00,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0xff, 0xff, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0,
    0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0xff, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x68, 0x90, 0x90, 0x90, 0x68, 0x00,
    0x30, 0x48, 0x48, 0x70, 0x48, 0x48, 0x70, 0xc0, 0xf8, 0x88, 0x80, 0x80,
    0x80, 0x80, 0x80, 0x00, 0xf8, 0x50, 0x50, 0x50, 0x50, 0x50, 0x98, 0x00,
    0xf8, 0x88, 0x40, 0x20, 0x40, 0x88, 0xf8, 0x00, 0x00, 0x00, 0x78, 0x90,
    0x90, 0x90, 0x60, 0x00, 0x00, 0x50, 0x50, 0x50, 0x50, 0x68, 0x80, 0x80,
    0x00, 0x50, 0xa0, 0x20, 0x20, 0x20, 0x20, 0x00, 0xf8, 0x20, 0x70, 0xa8,
    0xa8, 0x70, 0x20, 0xf8, 0x20, 0x50, 0x88, 0xf8, 0x88, 0x50, 0x20, 0x00,
    0x70, 0x88, 0x88, 0x88, 0x50, 0x50, 0xd8, 0x00, 0x30, 0x40, 0x40, 0x20,
    0x50, 0x50, 0x50, 0x20, 0x00, 0x00, 0x00, 0x50, 0xa8, 0xa8, 0x50, 0x00,
    0x08, 0x70, 0xa8, 0xa8, 0xa8, 0x70, 0x80, 0x00, 0x38, 0x40, 0x80, 0xf8,
    0x80, 0x40, 0x38, 0x00, 0x70, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x00,
    0x00, 0xf8, 0x00, 0xf8, 0x00, 0xf8, 0x00, 0x00, 0x20, 0x20, 0xf8, 0x20,
    0x20, 0x00, 0xf8, 0x00, 0xc0, 0x30, 0x08, 0x30, 0xc0, 0x00, 0xf8, 0x00,
    0x18, 0x60, 0x80, 0x60, 0x18, 0x00, 0xf8, 0x00, 0x10, 0x28, 0x20, 0x20,
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0xa0, 0x40,
    0x00, 0x20, 0x00, 0xf8, 0x00, 0x20, 0x00, 0x00, 0x00, 0x50, 0xa0, 0x00,
    0x50, 0xa0, 0x00, 0x00, 0x00, 0x18, 0x24, 0x24, 0x18, 0x00, 0x00, 0x00,
    0x00, 0x30, 0x78, 0x78, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x00, 0x00, 0x00, 0x3e, 0x20, 0x20, 0x20, 0xa0, 0x60, 0x20, 0x00,
    0xa0, 0x50, 0x50, 0x50, 0x00, 0x00, 0x00, 0x00, 0x40, 0xa0, 0x20, 0x40,
    0xe0, 0x00, 0x00, 0x00, 0x00, 0x38, 0x38, 0x38, 0x38, 0x38, 0x38, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
