use chrono::{DateTime, NaiveDate, TimeZone, Utc};

// The casts between the time types of chrono and those of MEOS. Each converts through the MEOS
// function stating the conversion, as `GoMEOS`'s functions/casts.go and the casts of every binding
// do (doc/specs/meos-bindings-algebra.md of MobilityDB, section 5): a value MEOS cannot state
// raises its error, which `meos_call!` reports, and a value chrono cannot state is refused here.

/// Casts a duration through `interval_from_parts`, its whole days as days and the rest as
/// microseconds, as `GoMEOS`'s `TimedeltaToInterval` casts a `time.Duration`.
pub(crate) fn create_interval(t: chrono::TimeDelta) -> meos_sys::Interval {
    let days = t.num_days();
    let rest = (t - chrono::TimeDelta::days(days))
        .num_microseconds()
        .expect("the part of a duration below a day is a count of microseconds");
    let days = i32::try_from(days).expect("a MEOS interval holds its days as a 32-bit count");
    let interval = unsafe { meos_call!(meos_sys::interval_from_parts(0, days, rest)) };
    let value = unsafe { interval.read() };
    unsafe { libc::free(interval.cast()) };
    value
}

/// Casts an interval through `interval_to_parts`, as `GoMEOS`'s `IntervalToTimedelta` casts one: an
/// interval with months has no fixed length, which a `chrono::TimeDelta` cannot state.
pub(crate) fn from_interval(interval: meos_sys::Interval) -> chrono::TimeDelta {
    let (mut months, mut days, mut us) = (0i32, 0i32, 0i64);
    unsafe {
        meos_call!(meos_sys::interval_to_parts(
            &raw const interval,
            &raw mut months,
            &raw mut days,
            &raw mut us
        ));
    }
    assert!(
        months == 0,
        "a chrono::TimeDelta cannot state an interval of {months} months"
    );
    chrono::TimeDelta::days(i64::from(days)) + chrono::TimeDelta::microseconds(us)
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

/// Casts an instant through `unix_us_to_timestamptz`, as `GoMEOS`'s `DatetimeToTimestamptz` does.
pub(crate) fn to_meos_timestamp<Tz: TimeZone>(dt: &DateTime<Tz>) -> i64 {
    unsafe { meos_call!(meos_sys::unix_us_to_timestamptz(dt.timestamp_micros())) }
}

/// Casts a timestamp with time zone through `timestamptz_to_unix_us`, in UTC, as `GoMEOS`'s
/// `TimestamptzToDatetime` does; MEOS refuses an infinite one, and an instant beyond the range of
/// a `DateTime` is refused here.
pub(crate) fn from_meos_timestamp(timestamp: meos_sys::TimestampTz) -> DateTime<Utc> {
    let us = unsafe { meos_call!(meos_sys::timestamptz_to_unix_us(timestamp)) };
    DateTime::from_timestamp_micros(us)
        .unwrap_or_else(|| panic!("a chrono::DateTime cannot state the instant of {us} microseconds"))
}

/// The first day chrono and MEOS count dates from in their conversions, 1970-01-01.
fn unix_epoch() -> NaiveDate {
    DateTime::UNIX_EPOCH.date_naive()
}

/// Casts a date through `unix_days_to_date`, as `GoMEOS`'s `DateToDateADT` does.
pub(crate) fn to_meos_date(date: NaiveDate) -> meos_sys::DateADT {
    let days = i32::try_from((date - unix_epoch()).num_days())
        .expect("a MEOS date holds its days as a 32-bit count");
    unsafe { meos_call!(meos_sys::unix_days_to_date(days)) }
}

/// Casts a date through `date_to_unix_days`, as `GoMEOS`'s `DateADTToDate` does; MEOS refuses an
/// infinite one.
pub(crate) fn from_meos_date(date: meos_sys::DateADT) -> NaiveDate {
    let days = unsafe { meos_call!(meos_sys::date_to_unix_days(date)) };
    unix_epoch()
        .checked_add_signed(chrono::TimeDelta::days(i64::from(days)))
        .unwrap_or_else(|| panic!("a chrono::NaiveDate cannot state the date of {days} days"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A duration crosses #create_interval as the interval of the same length, its whole days
    /// as days and the rest as microseconds, as `GoMEOS`'s `TimedeltaToInterval`
    /// (functions/casts.go) splits a `time.Duration`: a day is one day and no microseconds, and
    /// #from_interval reads it back as the same duration.
    #[test]
    fn a_day_is_one_day() {
        crate::meos_initialize();
        let interval = create_interval(chrono::TimeDelta::days(1));
        assert_eq!((interval.month, interval.day, interval.time), (0, 1, 0));
        assert_eq!(from_interval(interval), chrono::TimeDelta::days(1));
    }

    /// A span holds the dates and instants between its bounds, each crossing #to_meos_date or
    /// #to_meos_timestamp as the cast of its value, as #a_day_is_one_day reads an interval.
    #[test]
    fn a_span_contains_its_values() {
        use crate::collections::base::Collection;
        crate::meos_initialize();
        let date = |d| NaiveDate::from_ymd_opt(2023, 1, d).unwrap();
        let dates: crate::DateSpan = (date(1)..date(15)).into();
        assert!(dates.contains(&date(5)));
        assert!(!dates.contains(&date(20)));
        let dates: crate::DateSpanSet = "{[2023-01-01, 2023-01-15)}".parse().unwrap();
        assert!(dates.contains(&date(5)));
        let instant = |d| date(d).and_hms_opt(12, 0, 0).unwrap().and_utc();
        let instants: crate::TsTzSpan = (instant(1)..instant(15)).into();
        assert!(instants.contains(&instant(5)));
        assert!(!instants.contains(&instant(20)));
        let instants: crate::TsTzSpanSet =
            "{[2023-01-01 12:00:00+00, 2023-01-15 12:00:00+00)}".parse().unwrap();
        assert!(instants.contains(&instant(5)));
    }

    /// A duration below a day crosses #create_interval as its microseconds alone.
    #[test]
    fn an_hour_is_its_microseconds() {
        crate::meos_initialize();
        let interval = create_interval(chrono::TimeDelta::hours(1));
        assert_eq!((interval.month, interval.day, interval.time), (0, 0, 3_600_000_000));
    }
}
