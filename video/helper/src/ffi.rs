//! The shims' C entry points (`video/ddivideo.c`, `video/packets.c`,
//! `video/ddi_io.h`), built with `DDI_NATIVE`.
#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct Video {
    _private: [u8; 0],
}

#[repr(C)]
pub struct Demux {
    _private: [u8; 0],
}

pub type ReadFn =
    unsafe extern "C" fn(opaque: *mut c_void, offset: u64, buf: *mut u8, len: c_int) -> c_int;

unsafe extern "C" {
    pub fn ddi_io_set_read(read: ReadFn);
    pub fn ddi_set_threads(threads: c_int);

    pub fn ddi_open_io(opaque: *mut c_void, size: u64, hint: *const c_char) -> *mut Video;
    pub fn ddi_set_planes(v: *mut Video, buf: *mut u8, cap: usize);
    pub fn ddi_width(v: *mut Video) -> c_int;
    pub fn ddi_height(v: *mut Video) -> c_int;
    pub fn ddi_codec(v: *mut Video) -> *const c_char;
    pub fn ddi_colorspace(v: *mut Video) -> c_int;
    pub fn ddi_full_range(v: *mut Video) -> c_int;
    pub fn ddi_planes_size(v: *mut Video) -> usize;
    pub fn ddi_last_time(v: *mut Video) -> f64;
    pub fn ddi_duration(v: *mut Video) -> f64;
    pub fn ddi_next(v: *mut Video) -> f64;
    pub fn ddi_seek(v: *mut Video, t: f64) -> c_int;
    pub fn ddi_seek_key(v: *mut Video, t: f64) -> c_int;
    pub fn ddi_close(v: *mut Video);

    pub fn ddi_demux_abi() -> c_int;
    pub fn ddi_demux_open_io(opaque: *mut c_void, size: u64, hint: *const c_char) -> *mut Demux;
    pub fn ddi_demux_close(d: *mut Demux);
    pub fn ddi_demux_info(d: *mut Demux) -> *const c_char;
    pub fn ddi_demux_error(d: *mut Demux) -> *const c_char;
    pub fn ddi_demux_config(d: *mut Demux) -> *const u8;
    pub fn ddi_demux_config_size(d: *mut Demux) -> u32;
    pub fn ddi_demux_packet_info(d: *mut Demux) -> *const u8;
    pub fn ddi_demux_data(d: *mut Demux) -> *const u8;
    pub fn ddi_demux_side_data(d: *mut Demux) -> *const u8;
    pub fn ddi_demux_next(d: *mut Demux) -> c_int;
    pub fn ddi_demux_seek(d: *mut Demux, timestamp: i64) -> c_int;
    pub fn ddi_demux_index_count(d: *mut Demux) -> c_int;
    pub fn ddi_demux_index(d: *mut Demux, first: u32, count: u32) -> c_int;
    pub fn ddi_demux_index_data(d: *mut Demux) -> *const u8;

    // libavutil
    pub fn av_max_alloc(max: usize);
    pub fn av_log_set_level(level: c_int);
    pub fn av_version_info() -> *const c_char;
}
