//! C ABI for the NSWindow host. No PTY symbols.
//!
//! T-NFR's timing lives on the ObjC side (ADR 0003 D5): the segment starts at
//! an `NSEvent` and ends at a drawable presentation. What crosses here are the
//! oracle primitives — cursor cell, cell contents, warm-path frame accounting.

use crate::{load_surface, Client};
use rill_chip0::PodCell;
use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::Mutex;

static LAST_ERR: Mutex<Option<CString>> = Mutex::new(None);

fn set_err(msg: impl ToString) {
    if let Ok(mut g) = LAST_ERR.lock() {
        *g = CString::new(msg.to_string()).ok();
    }
}

#[no_mangle]
pub extern "C" fn rill_client_last_error() -> *const c_char {
    match LAST_ERR.lock() {
        Ok(g) => g.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
        Err(_) => ptr::null(),
    }
}

/// # Safety
/// `socket` is NUL-terminated UTF-8 or NULL.
#[no_mangle]
pub unsafe extern "C" fn rill_client_connect(socket: *const c_char) -> *mut Client {
    let path = if socket.is_null() {
        crate::default_socket()
    } else {
        let c = unsafe { CStr::from_ptr(socket) };
        std::path::PathBuf::from(c.to_string_lossy().as_ref())
    };
    let surface = match load_surface() {
        Ok(s) => s,
        Err(e) => {
            set_err(e);
            return ptr::null_mut();
        }
    };
    match Client::connect(&path, surface) {
        Ok(c) => Box::into_raw(Box::new(c)),
        Err(e) => {
            set_err(e);
            ptr::null_mut()
        }
    }
}

/// # Safety
/// `client` came from `rill_client_connect` and is not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn rill_client_free(client: *mut Client) {
    if !client.is_null() {
        unsafe { drop(Box::from_raw(client)) };
    }
}

/// The attach socket fd, for arming a `dispatch_source` (SPEC-DISPLAY §3).
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_socket_fd(client: *const Client) -> i32 {
    if client.is_null() {
        return -1;
    }
    unsafe { (*client).socket_fd() }
}

/// # Safety
/// `bytes` points to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rill_client_send_input(
    client: *mut Client,
    bytes: *const u8,
    len: usize,
) -> i32 {
    if client.is_null() || bytes.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    match c.send_input(slice) {
        Ok(()) => 0,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_resize(
    client: *mut Client,
    cols: u16,
    rows: u16,
    px_w: u16,
    px_h: u16,
) -> i32 {
    if client.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.resize(cols, rows, px_w, px_h) {
        Ok(()) => 0,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// Returns bytes fed this turn, or -1 on error.
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_pump(client: *mut Client) -> isize {
    if client.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.pump() {
        Ok(n) => n as isize,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_alive(client: *const Client) -> i32 {
    if client.is_null() {
        return 0;
    }
    i32::from(unsafe { (*client).alive() })
}

/// Raw wait status, or `INT32_MIN` while the child is alive.
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_exit_status(client: *const Client) -> i32 {
    if client.is_null() {
        return i32::MIN;
    }
    unsafe { (*client).exit_status() }.unwrap_or(i32::MIN)
}

/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_font_family(client: *const Client) -> *const c_char {
    if client.is_null() {
        return ptr::null();
    }
    let c = unsafe { &*client };
    thread_local! {
        static BUF: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
    }
    BUF.with(|b| {
        *b.borrow_mut() = CString::new(c.font_family()).ok();
        b.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(ptr::null())
    })
}

/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_font_size(client: *const Client) -> f32 {
    if client.is_null() {
        return 13.0;
    }
    unsafe { (*client).font_size() }
}

#[repr(C)]
pub struct CPodGrid {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cursor_visible: u8,
    pub full_damage: u8,
    pub damage_row0: u16,
    pub damage_row1: u16,
    pub grapheme_truncated: u32,
    pub cells: *const PodCell,
    pub ncells: usize,
}

/// # Safety
/// `out` points to a writable `CPodGrid`. `out->cells` borrows thread-local
/// storage valid until the next call on this thread.
#[no_mangle]
pub unsafe extern "C" fn rill_client_snapshot(client: *mut Client, out: *mut CPodGrid) -> i32 {
    if client.is_null() || out.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.snapshot() {
        Ok(grid) => {
            thread_local! {
                static CELLS: std::cell::RefCell<Vec<PodCell>> =
                    const { std::cell::RefCell::new(Vec::new()) };
            }
            CELLS.with(|holder| {
                let mut v = holder.borrow_mut();
                *v = grid.cells;
                unsafe {
                    (*out).cols = grid.cols;
                    (*out).rows = grid.rows;
                    (*out).cursor_col = grid.cursor_col;
                    (*out).cursor_row = grid.cursor_row;
                    (*out).cursor_visible = u8::from(grid.cursor_visible);
                    (*out).full_damage = u8::from(grid.full_damage);
                    (*out).damage_row0 = grid.damage_row0;
                    (*out).damage_row1 = grid.damage_row1;
                    (*out).grapheme_truncated = grid.grapheme_truncated;
                    (*out).cells = v.as_ptr();
                    (*out).ncells = v.len();
                }
            });
            0
        }
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

// ------------------------------------------------------- T-NFR oracle support

/// Codepoint at a specific cell, or `0` if out of range.
///
/// T-NFR's sentinel is cell-position specific: the old oracle scanned the whole
/// grid for a letter the shell had already echoed there on a previous cycle, so
/// it completed without any PTY round trip (ADR 0003 D6, audit S1-2).
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_cell_codepoint(
    client: *mut Client,
    col: u16,
    row: u16,
) -> u32 {
    if client.is_null() {
        return 0;
    }
    let c = unsafe { &mut *client };
    match c.snapshot() {
        Ok(g) => g.cell(col, row).map(|x| x.codepoint).unwrap_or(0),
        Err(_) => 0,
    }
}

/// Writes the cursor cell. Returns 0 on success.
///
/// # Safety
/// `col` and `row` point to writable `uint16_t`.
#[no_mangle]
pub unsafe extern "C" fn rill_client_cursor(
    client: *mut Client,
    col: *mut u16,
    row: *mut u16,
) -> i32 {
    if client.is_null() || col.is_null() || row.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.snapshot() {
        Ok(g) => {
            unsafe {
                *col = g.cursor_col;
                *row = g.cursor_row;
            }
            0
        }
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// Start counting frames that do not belong on a keystroke.
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_begin_warm_path_audit(client: *mut Client) {
    if !client.is_null() {
        unsafe { (*client).begin_warm_path_audit() };
    }
}

/// Frames sent that were not DATA/CREDIT, plus non-DATA frames received.
///
/// This replaces the previous check, which grepped the attach byte stream for
/// `pane_replay` and `"cells"` — strings a tag+length binary protocol cannot
/// contain, so `control_rpc=0` was guaranteed by the format (audit S1-3).
///
/// # Safety
/// `client` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn rill_client_end_warm_path_audit(client: *mut Client) -> u32 {
    if client.is_null() {
        return u32::MAX;
    }
    unsafe { (*client).end_warm_path_audit() }
}
