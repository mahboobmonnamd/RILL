//! C ABI for the NSWindow host. No PTY symbols.

use crate::{load_surface, nfr_key, Client};
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

#[no_mangle]
pub extern "C" fn rill_client_connect(socket: *const c_char) -> *mut Client {
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

#[no_mangle]
pub extern "C" fn rill_client_free(client: *mut Client) {
    if !client.is_null() {
        unsafe {
            drop(Box::from_raw(client));
        }
    }
}

#[no_mangle]
pub extern "C" fn rill_client_send_input(client: *mut Client, bytes: *const u8, len: usize) -> i32 {
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

#[no_mangle]
pub extern "C" fn rill_client_resize(
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

#[no_mangle]
pub extern "C" fn rill_client_pump(client: *mut Client) -> i32 {
    if client.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.pump() {
        Ok(_) => 0,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn rill_client_alive(client: *const Client) -> i32 {
    if client.is_null() {
        return 0;
    }
    let c = unsafe { &*client };
    i32::from(c.alive())
}

#[no_mangle]
pub extern "C" fn rill_client_font_family(client: *const Client) -> *const c_char {
    if client.is_null() {
        return ptr::null();
    }
    let c = unsafe { &*client };
    thread_local! {
        static BUF: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
    }
    BUF.with(|b| {
        *b.borrow_mut() = CString::new(c.font_family()).ok();
        b.borrow().as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null())
    })
}

#[no_mangle]
pub extern "C" fn rill_client_font_size(client: *const Client) -> f32 {
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
    pub cells: *const PodCell,
    pub ncells: usize,
}

#[no_mangle]
pub extern "C" fn rill_client_snapshot(client: *mut Client, out: *mut CPodGrid) -> i32 {
    if client.is_null() || out.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match c.snapshot() {
        Ok(grid) => {
            thread_local! {
                static CELLS: std::cell::RefCell<Vec<PodCell>> = const { std::cell::RefCell::new(Vec::new()) };
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

#[no_mangle]
pub extern "C" fn rill_client_nfr_key(
    client: *mut Client,
    count: u32,
    p95_ms: *mut f64,
    control_rpc: *mut i32,
    on_battery: *mut i32,
) -> i32 {
    if client.is_null() {
        return -1;
    }
    let c = unsafe { &mut *client };
    match nfr_key(c, count.max(1)) {
        Ok(r) => {
            if !p95_ms.is_null() {
                unsafe { *p95_ms = r.p95_ms };
            }
            if !control_rpc.is_null() {
                unsafe { *control_rpc = i32::from(r.control_rpc) };
            }
            if !on_battery.is_null() {
                unsafe { *on_battery = i32::from(r.on_battery) };
            }
            0
        }
        Err(e) => {
            set_err(e);
            -1
        }
    }
}
