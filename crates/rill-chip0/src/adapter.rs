//! FFI to the C adapter. Ghostty types are not named here.

use crate::{Error, PodCell, PodGrid};
use std::ptr;

#[repr(C)]
struct CHeader {
    cols: u16,
    rows: u16,
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: u8,
    full_damage: u8,
    damage_row0: u16,
    damage_row1: u16,
    /// Must mirror `RillPodHeader` in `adapter/rill_chip0_vt.h`.
    grapheme_truncated: u32,
}

#[repr(C)]
struct CCell {
    codepoint: u32,
    fg: u32,
    bg: u32,
    attrs: u16,
    _pad: u16,
}

extern "C" {
    fn rill_vt_new(out: *mut *mut std::ffi::c_void, cols: u16, rows: u16) -> i32;
    fn rill_vt_free(vt: *mut std::ffi::c_void);
    fn rill_vt_feed(vt: *mut std::ffi::c_void, data: *const u8, len: usize);
    fn rill_vt_resize(
        vt: *mut std::ffi::c_void,
        cols: u16,
        rows: u16,
        cell_w: u32,
        cell_h: u32,
    ) -> i32;
    fn rill_vt_snapshot(
        vt: *mut std::ffi::c_void,
        hdr: *mut CHeader,
        cells: *mut *mut CCell,
        ncells: *mut usize,
    ) -> i32;
    fn rill_vt_repaint_bytes(
        vt: *mut std::ffi::c_void,
        bytes: *mut *mut u8,
        len: *mut usize,
    ) -> i32;
    fn rill_vt_reset(vt: *mut std::ffi::c_void);
    fn rill_vt_buf_free(ptr: *mut u8, len: usize);
    fn rill_vt_cells_free(ptr: *mut CCell);
}

pub struct Vt {
    ptr: *mut std::ffi::c_void,
}

unsafe impl Send for Vt {}

impl Vt {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        let mut ptr = ptr::null_mut();
        let rc = unsafe { rill_vt_new(&mut ptr, cols, rows) };
        if rc != 0 || ptr.is_null() {
            return Err(Error::Vt("new"));
        }
        Ok(Self { ptr })
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        unsafe { rill_vt_feed(self.ptr, bytes.as_ptr(), bytes.len()) }
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error> {
        let rc = unsafe { rill_vt_resize(self.ptr, cols, rows, cell_w, cell_h) };
        if rc != 0 {
            return Err(Error::Vt("resize"));
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        unsafe { rill_vt_reset(self.ptr) }
    }

    pub fn snapshot(&mut self) -> Result<PodGrid, Error> {
        let mut hdr = CHeader {
            cols: 0,
            rows: 0,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: 0,
            full_damage: 0,
            damage_row0: 0,
            damage_row1: 0,
            grapheme_truncated: 0,
        };
        let mut cells_ptr: *mut CCell = ptr::null_mut();
        let mut n = 0usize;
        let rc = unsafe { rill_vt_snapshot(self.ptr, &mut hdr, &mut cells_ptr, &mut n) };
        if rc != 0 {
            return Err(Error::Vt("snapshot"));
        }
        let slice = if cells_ptr.is_null() || n == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(cells_ptr, n) }
        };
        let cells = slice
            .iter()
            .map(|c| PodCell {
                codepoint: c.codepoint,
                fg: c.fg,
                bg: c.bg,
                attrs: c.attrs,
                _pad: 0,
            })
            .collect();
        if !cells_ptr.is_null() {
            unsafe { rill_vt_cells_free(cells_ptr) }
        }
        Ok(PodGrid {
            cols: hdr.cols,
            rows: hdr.rows,
            cursor_col: hdr.cursor_col,
            cursor_row: hdr.cursor_row,
            cursor_visible: hdr.cursor_visible != 0,
            full_damage: hdr.full_damage != 0,
            damage_row0: hdr.damage_row0,
            damage_row1: hdr.damage_row1,
            grapheme_truncated: hdr.grapheme_truncated,
            cells,
        })
    }

    pub fn repaint_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let mut ptr: *mut u8 = ptr::null_mut();
        let mut len = 0usize;
        let rc = unsafe { rill_vt_repaint_bytes(self.ptr, &mut ptr, &mut len) };
        if rc != 0 {
            return Err(Error::Vt("repaint"));
        }
        let bytes = if ptr.is_null() || len == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
        };
        if !ptr.is_null() {
            unsafe { rill_vt_buf_free(ptr, len) }
        }
        Ok(bytes)
    }
}

impl Drop for Vt {
    fn drop(&mut self) {
        unsafe { rill_vt_free(self.ptr) }
    }
}
