use chrono::{DateTime, TimeZone, Utc};

use crate::collections::datetime::MICROSECONDS_UNTIL_2000;

pub(crate) fn create_interval(t: chrono::TimeDelta) -> meos_sys::Interval {
    let time_in_microseconds = t.num_microseconds().unwrap_or(0);
    let total_days = t.num_days() as i32;

    meos_sys::Interval {
        time: time_in_microseconds,
        day: total_days,
        month: 0,
    }
}

pub(crate) fn from_interval(interval: meos_sys::Interval) -> chrono::TimeDelta {
    let time_in_microseconds = interval.time;
    let days = i64::from(interval.day);
    let months = i64::from(interval.month);

    chrono::TimeDelta::microseconds(time_in_microseconds)
        + chrono::TimeDelta::days(days + months * 30) // meos assumes 30 days per month
}

/// Reads an interval MEOS allocated as a `TimeDelta`, then frees it.
pub(crate) fn from_owned_interval(interval: *mut meos_sys::Interval) -> chrono::TimeDelta {
    let delta = from_interval(unsafe { interval.read() });
    unsafe { libc::free(interval.cast()) };
    delta
}

/// Reads a string MEOS allocated by the size it states, its terminator included, then frees
/// it, as `from_owned_interval` reads an interval; `None` when the bytes are not UTF-8.
///
/// # Safety
/// `string` is a string MEOS returned with `size` as its `size_out`, not yet freed.
pub(crate) unsafe fn take_meos_string(string: *mut std::ffi::c_char, size: usize) -> Option<String> {
    let bytes = unsafe { std::slice::from_raw_parts(string.cast::<u8>(), size.saturating_sub(1)) };
    let owned = std::str::from_utf8(bytes).ok().map(str::to_owned);
    unsafe { libc::free(string.cast()) };
    owned
}

pub(crate) fn to_meos_timestamp<Tz: TimeZone>(dt: &DateTime<Tz>) -> i64 {
    dt.timestamp_micros() - MICROSECONDS_UNTIL_2000
}

pub(crate) fn from_meos_timestamp(timestamp: meos_sys::TimestampTz) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(timestamp + MICROSECONDS_UNTIL_2000)
        .expect("Failed to parse DateTime")
}
