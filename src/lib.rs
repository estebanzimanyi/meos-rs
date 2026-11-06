#![crate_name = "meos"]
#![crate_type = "lib"]
#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![doc(html_logo_url = "https://libmeos.org/brand.svg")]
#![allow(refining_impl_trait)]
#![allow(clippy::non_canonical_partial_ord_impl)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_sign_loss)]

use crate::utils::take_meos_string;
use std::{
    ffi::{CStr, CString},
    fmt::Debug,
    sync::Once,
};

use bitmask_enum::bitmask;
use boxes::Box as MeosBox;
pub use meos_sys as sys;

thread_local! {
    /// The message of the last MEOS error raised on this thread and not yet reported,
    /// which `#error_handler` records and `#check_meos_error` reports.
    static MEOS_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Panic with the message of the MEOS error the call just made raised, if any. The
/// panic is raised here, in Rust, after the C call has returned, so it unwinds as any
/// Rust panic does; raised inside the C `#error_handler` it would abort the process.
pub(crate) fn check_meos_error() {
    if let Some(message) = MEOS_ERROR.with(|e| e.borrow_mut().take()) {
        panic!("{message}");
    }
}

/// Call a MEOS function and report the error it raised (`#check_meos_error`), the way
/// every call of the crate into `meos_sys` is made. A crate-internal macro as
/// `#impl_collection` of `collections/base/collection.rs` is, defined before the
/// modules that use it.
macro_rules! meos_call {
    ($call:expr) => {{
        let result = $call;
        $crate::check_meos_error();
        result
    }};
}

mod boxes;
pub use boxes::{Box, STBox, TBox};

mod collections;
pub use collections::base::{Collection, Span, SpanSet};
pub use collections::datetime::{DateSpan, DateSpanSet, TsTzSpan, TsTzSpanSet};
pub use collections::number::*;

mod errors;
pub use errors::ParseError;

mod temporal;
pub use temporal::*;

pub(crate) mod utils;

static START: Once = Once::new();

#[allow(dead_code)]
extern "C" fn finalize() {
    unsafe {
        meos_call!(meos_sys::meos_finalize());
    }
}

pub trait BoundingBox: Collection {}

impl<T> BoundingBox for T where T: MeosBox {}

/// The MEOS error handler: it records the message of an error for `check_meos_error`
/// and returns, so the MEOS function returns its sentinel to the call `meos_call!`
/// made. A message of a lower level accompanies an answer and is not an error.
unsafe extern "C" fn error_handler(
    error_level: std::os::raw::c_int,
    _error_code: std::os::raw::c_int,
    message: *const std::os::raw::c_char,
) {
    if error_level < meos_sys::errorLevel_MEOS_ERROR as std::os::raw::c_int {
        return;
    }
    let text = if message.is_null() {
        String::from("MEOS error")
    } else {
        CStr::from_ptr(message).to_string_lossy().into_owned()
    };
    MEOS_ERROR.with(|e| *e.borrow_mut() = Some(text));
}

/// Initializes the underlying MEOS platform.
///
/// This function must be called before any other PyMEOS-related function is used.
///
/// # Arguments
///
/// * `tz` - A string slice (`&str`) indicating the desired timezone to be used.
///
/// ## Example
/// ```
/// # use meos::meos_initialize;
///
/// meos_initialize();
/// ```
pub fn meos_initialize() {
    START.call_once(|| unsafe {
        #[cfg(feature = "bundled")]
        {
            let path = CString::new(env!("MEOS_SPATIAL_REF_SYS_CSV")).unwrap();
            meos_call!(meos_sys::meos_set_spatial_ref_sys_csv(path.as_ptr()));
        }
        meos_call!(meos_sys::meos_initialize());
        meos_call!(meos_sys::meos_initialize_error_handler(Some(error_handler)));
    });
}

pub fn meos_initialize_timezone(tz: &str) {
    unsafe {
        #[allow(clippy::missing_panics_doc)]
        let ptr = CString::new(tz).expect("Wrong timezone format");

        meos_call!(meos_sys::meos_initialize_timezone(ptr.as_ptr()));
    }
}

fn factory<T: MeosEnum>(temporal: *mut meos_sys::Temporal) -> T {
    let temporal_type: TemporalSubtype = unsafe { u32::from(temporal.read().subtype).into() };
    match temporal_type {
        TemporalSubtype::Instant => T::from_instant(temporal.cast()),
        TemporalSubtype::Sequence => T::from_sequence(temporal.cast()),
        TemporalSubtype::SequenceSet => T::from_sequence_set(temporal.cast()),
        TemporalSubtype::Any => unreachable!(),
    }
}

#[bitmask(u8)]
pub enum WKBVariant {
    /// Little endian encoding
    NDR = meos_sys::WKB_NDR as u8,
    /// Big endian encoding
    XDR = meos_sys::WKB_XDR as u8,
    /// Extended variant
    Extended = meos_sys::WKB_EXTENDED as u8,
}

#[derive(Debug, PartialEq)]
pub enum TemporalSubtype {
    Any = meos_sys::tempSubtype_ANYTEMPSUBTYPE as isize,
    Instant = meos_sys::tempSubtype_TINSTANT as isize,
    Sequence = meos_sys::tempSubtype_TSEQUENCE as isize,
    SequenceSet = meos_sys::tempSubtype_TSEQUENCESET as isize,
}

impl From<u32> for TemporalSubtype {
    fn from(value: u32) -> Self {
        match value {
            meos_sys::tempSubtype_TINSTANT => TemporalSubtype::Instant,
            meos_sys::tempSubtype_TSEQUENCE => TemporalSubtype::Sequence,
            meos_sys::tempSubtype_TSEQUENCESET => TemporalSubtype::SequenceSet,
            _ => TemporalSubtype::Any, // default case, as it's often the case for "unknown" or "any"
        }
    }
}

pub trait MeosEnum: Debug + Sized + Temporal {
    fn from_instant(inner: *mut meos_sys::TInstant) -> Self;
    fn from_sequence(inner: *mut meos_sys::TSequence) -> Self;
    fn from_sequence_set(inner: *mut meos_sys::TSequenceSet) -> Self;

    /// Creates a temporal object from an MF-JSON string.
    ///
    /// ## Arguments
    /// * `mfjson` - The MF-JSON string.
    ///
    /// ## Returns
    /// A temporal object.
    fn from_mfjson(mfjson: &str) -> Self;

    /// Creates a temporal object from Well-Known Binary (WKB) bytes.
    ///
    /// ## Arguments
    /// * `wkb` - The WKB bytes.
    ///
    /// ## Returns
    /// A temporal object.
    fn from_wkb(wkb: &[u8]) -> Self {
        factory::<Self>(unsafe { meos_call!(meos_sys::temporal_from_wkb(wkb.as_ptr(), wkb.len())) })
    }

    /// Creates a temporal object from a hex-encoded WKB string.
    ///
    /// ## Arguments
    /// * `hexwkb` - The hex-encoded WKB string.
    ///
    /// ## Returns
    /// A temporal object.
    fn from_hexwkb(hexwkb: &[u8]) -> Self {
        let c_hexwkb = CString::new(hexwkb).unwrap();
        unsafe {
            let inner = meos_call!(meos_sys::temporal_from_hexwkb(c_hexwkb.as_ptr()));
            factory::<Self>(inner)
        }
    }

    /// Creates a temporal object by merging multiple temporal objects.
    ///
    /// ## Arguments
    /// * `temporals` - The temporal objects to merge.
    ///
    /// ## Returns
    /// A merged temporal object.
    fn from_merge(temporals: &[Self]) -> Self {
        let mut t_list: Vec<*mut meos_sys::Temporal> = temporals
            .iter()
            .map(|t| Self::inner(t).cast_mut())
            .collect();
        factory::<Self>(unsafe {
            meos_call!(meos_sys::temporal_merge_array(t_list.as_mut_ptr(), temporals.len() as i32))
        })
    }

    /// Returns the temporal object as an MF-JSON string.
    ///
    /// ## Arguments
    /// * `with_bbox` - Whether to include the bounding box in the output.
    /// * `flags` - The flags to use for the output.
    /// * `precision` - The precision to use for the output.
    /// * `srs` - The spatial reference system (SRS) to use for the output.
    ///
    /// ## Returns
    /// The temporal object as an MF-JSON string.
    fn as_mfjson(
        &self,
        with_bbox: bool,
        variant: JSONCVariant,
        precision: i32,
        srs: &str,
    ) -> String {
        let srs = CString::new(srs).unwrap();
        let mut size = 0;
        let out_str = unsafe {
            meos_call!(meos_sys::temporal_as_mfjson(
                self.inner(),
                i32::from(with_bbox),
                variant as i32,
                precision,
                srs.as_ptr(),
                &raw mut size,
            ))
        };
        unsafe { take_meos_string(out_str, size) }.unwrap()
    }

    /// Returns the temporal object as Well-Known Binary (WKB) bytes.
    ///
    /// ## Returns
    /// The temporal object as WKB bytes.
    fn as_wkb(&self, variant: WKBVariant) -> &[u8] {
        unsafe {
            let mut size: usize = 0;
            let ptr = meos_call!(meos_sys::temporal_as_wkb(self.inner(), variant.into(), &raw mut size));
            std::slice::from_raw_parts(ptr, size)
        }
    }

    /// Returns the temporal object as a hex-encoded WKB string.
    ///
    /// ## Returns
    /// The temporal object as a hex-encoded WKB bytes.
    fn as_hexwkb(&self, variant: WKBVariant) -> &[u8] {
        unsafe {
            let mut size: usize = 0;
            let hexwkb_ptr =
                meos_call!(meos_sys::temporal_as_hexwkb(self.inner(), variant.into(), &raw mut size));

            CStr::from_ptr(hexwkb_ptr).to_bytes()
        }
    }
}

macro_rules! impl_from_str {
    ($type:ty) => {
        paste::paste! {
        impl FromStr for $type {
            type Err = ParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                CString::new(s).map_err(|_| ParseError).map(|string| {
                    let inner = unsafe { meos_call!(meos_sys::[<$type:lower _in>](string.as_ptr())) };
                    factory::<Self>(inner)
                })
            }
        }}
    };
}

pub(crate) use impl_from_str;

#[cfg(test)]
#[serial_test::serial]
mod error_tests {
    use std::str::FromStr;

    use crate::{meos_initialize, TBox};

    /// A MEOS error reaches the caller as a Rust panic it can catch, carrying the MEOS
    /// message, and leaves no error behind for the next call (`#check_meos_error`).
    #[test]
    fn a_meos_error_is_a_catchable_panic() {
        meos_initialize();
        let caught = std::panic::catch_unwind(|| TBox::from_str("not a box"));
        let payload = caught.expect_err("a malformed box raises a MEOS error");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        assert!(!message.is_empty(), "the panic carries the MEOS message");
        assert!(TBox::from_str("TBOXINT X([1, 2))").is_ok());
    }
}
