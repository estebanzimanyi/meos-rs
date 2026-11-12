use std::ffi::CString;

use crate::{errors::ParseError, utils::take_meos_string, WKBVariant};

/// Defines a grid cell type: a newtype over the `u64` MEOS passes the cell as, read and
/// written through the MEOS functions of its `$prefix` as `#TBox` reads and writes a box
/// (`from_wkb`, `from_hexwkb`, `as_wkb`, `as_hexwkb`, `FromStr` through `*_in`), a
/// crate-internal macro as `#impl_collection` is. The writers return owned bytes and
/// strings, and the default string is the text `*_out` writes. A cell carries no SRID, so it
/// has no EWKT, EWKB or SRID forms.
macro_rules! impl_cell {
    ($(#[$doc:meta])* $name:ident, $prefix:ident) => {
        paste::paste! {
            $(#[$doc])*
            #[derive(Clone, Copy, PartialEq, Eq, Hash)]
            pub struct $name(u64);

            impl $name {
                /// Wraps the `u64` MEOS holds the cell as.
                pub fn from_inner(inner: u64) -> Self {
                    Self(inner)
                }

                /// Returns the `u64` MEOS holds the cell as.
                pub fn inner(&self) -> u64 {
                    self.0
                }

                /// Reads a cell from its WKB.
                pub fn from_wkb(wkb: &[u8]) -> Self {
                    Self(unsafe { meos_call!(meos_sys::[<$prefix _from_wkb>](wkb.as_ptr(), wkb.len())) })
                }

                /// Reads a cell from its hex-encoded WKB.
                pub fn from_hexwkb(hexwkb: &[u8]) -> Self {
                    let c_hexwkb = CString::new(hexwkb).unwrap();
                    Self(unsafe { meos_call!(meos_sys::[<$prefix _from_hexwkb>](c_hexwkb.as_ptr())) })
                }

                /// Writes the cell as WKB in the given variant.
                pub fn as_wkb(&self, variant: WKBVariant) -> Vec<u8> {
                    unsafe {
                        let mut size: usize = 0;
                        let ptr = meos_call!(meos_sys::[<$prefix _as_wkb>](self.0, variant.into(), &raw mut size));
                        let wkb = std::slice::from_raw_parts(ptr, size).to_vec();
                        libc::free(ptr.cast());
                        wkb
                    }
                }

                /// Writes the cell as hex-encoded WKB in the given variant.
                pub fn as_hexwkb(&self, variant: WKBVariant) -> String {
                    let mut size: usize = 0;
                    let ptr = unsafe {
                        meos_call!(meos_sys::[<$prefix _as_hexwkb>](self.0, variant.into(), &raw mut size))
                    };
                    unsafe { take_meos_string(ptr, size) }.expect("MEOS writes hex WKB as ASCII")
                }
            }

            impl std::str::FromStr for $name {
                type Err = ParseError;

                /// Reads a cell from its text.
                fn from_str(string: &str) -> Result<Self, Self::Err> {
                    CString::new(string)
                        .map_err(|_| ParseError)
                        .map(|string| Self(unsafe { meos_call!(meos_sys::[<$prefix _in>](string.as_ptr())) }))
                }
            }

            impl std::fmt::Display for $name {
                /// Writes the cell as its text.
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let mut size: usize = 0;
                    let ptr = unsafe { meos_call!(meos_sys::[<$prefix _out>](self.0, &raw mut size)) };
                    let string = unsafe { take_meos_string(ptr, size) }.ok_or(std::fmt::Error)?;
                    f.write_str(&string)
                }
            }

            impl std::fmt::Debug for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::Display::fmt(self, f)
                }
            }
        }
    };
}

impl_cell!(
    /// An H3 cell, held by value as the `u64` of its index.
    ///
    /// ## Example
    /// ```
    /// # use meos::meos_initialize;
    /// # meos_initialize();
    /// use meos::H3Index;
    /// let cell: H3Index = "8a2a1072b59ffff".parse().unwrap();
    /// assert_eq!(cell.to_string(), "8a2a1072b59ffff");
    /// ```
    H3Index,
    h3index
);

impl_cell!(
    /// A Quadbin cell, held by value as the `u64` of its index.
    ///
    /// ## Example
    /// ```
    /// # use meos::meos_initialize;
    /// # meos_initialize();
    /// use meos::{Quadbin, WKBVariant};
    /// let cell: Quadbin = "48427fffffffffff".parse().unwrap();
    /// assert_eq!(cell.as_hexwkb(WKBVariant::NDR), "01470000FFFFFFFFFF7F4248");
    /// ```
    Quadbin,
    quadbin
);

impl_cell!(
    /// An S2 cell, held by value as the `u64` of its cell identifier.
    ///
    /// ## Example
    /// ```
    /// # use meos::meos_initialize;
    /// # meos_initialize();
    /// use meos::{S2Cell, WKBVariant};
    /// let cell: S2Cell = "47c3c444c".parse().unwrap();
    /// assert_eq!(cell.as_hexwkb(WKBVariant::NDR), "01550000000000C044C4C347");
    /// ```
    S2Cell,
    s2cell
);

#[cfg(test)]
#[serial_test::serial]
mod tests {
    use std::str::FromStr;

    use super::{H3Index, Quadbin, S2Cell};
    use crate::{meos_initialize, WKBVariant};

    // Expected values from MobilityDB's regression outputs: `test/h3/expected/250_h3index.test.out`
    // and `test/io/expected/700_io_every_type_tbl.test.out`.

    #[test]
    fn h3index_reads_and_writes_its_text_and_wkb() {
        meos_initialize();
        let cell = H3Index::from_str("8a2a1072b59ffff").unwrap();
        assert_eq!(cell.to_string(), "8a2a1072b59ffff");
        assert_eq!(H3Index::from_str("0x8a2a1072b59ffff").unwrap(), cell);
        assert_eq!(H3Index::from_wkb(&cell.as_wkb(WKBVariant::NDR)), cell);
        assert_eq!(H3Index::from_wkb(&cell.as_wkb(WKBVariant::XDR)), cell);
        assert_eq!(
            H3Index::from_hexwkb(cell.as_hexwkb(WKBVariant::NDR).as_bytes()),
            cell
        );
    }

    #[test]
    fn quadbin_reads_and_writes_its_text_and_wkb() {
        meos_initialize();
        let cell = Quadbin::from_str("48427fffffffffff").unwrap();
        assert_eq!(cell.to_string(), "48427fffffffffff");
        assert_eq!(
            Quadbin::from_str("0x480fffffffffffff").unwrap().to_string(),
            "480fffffffffffff"
        );
        assert_eq!(
            cell.as_wkb(WKBVariant::NDR),
            [0x01, 0x47, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x42, 0x48]
        );
        assert_eq!(
            cell.as_wkb(WKBVariant::XDR),
            [0x00, 0x00, 0x47, 0x00, 0x48, 0x42, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(cell.as_hexwkb(WKBVariant::NDR), "01470000FFFFFFFFFF7F4248");
        assert_eq!(cell.as_hexwkb(WKBVariant::XDR), "0000470048427FFFFFFFFFFF");
        assert_eq!(Quadbin::from_wkb(&cell.as_wkb(WKBVariant::NDR)), cell);
        assert_eq!(Quadbin::from_hexwkb(b"0000470048427FFFFFFFFFFF"), cell);
    }

    #[test]
    fn s2cell_reads_and_writes_its_text_and_wkb() {
        meos_initialize();
        let cell = S2Cell::from_str("47c3c444c").unwrap();
        assert_eq!(cell.to_string(), "47c3c444c");
        assert_eq!(S2Cell::from_str("47c3c444c0000000").unwrap(), cell);
        assert_eq!(
            cell.as_wkb(WKBVariant::NDR),
            [0x01, 0x55, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x44, 0xc4, 0xc3, 0x47]
        );
        assert_eq!(
            cell.as_wkb(WKBVariant::XDR),
            [0x00, 0x00, 0x55, 0x00, 0x47, 0xc3, 0xc4, 0x44, 0xc0, 0x00, 0x00, 0x00]
        );
        assert_eq!(cell.as_hexwkb(WKBVariant::NDR), "01550000000000C044C4C347");
        assert_eq!(cell.as_hexwkb(WKBVariant::XDR), "0000550047C3C444C0000000");
        assert_eq!(S2Cell::from_wkb(&cell.as_wkb(WKBVariant::XDR)), cell);
        assert_eq!(S2Cell::from_hexwkb(b"01550000000000C044C4C347"), cell);
    }

    #[test]
    fn a_malformed_cell_is_a_meos_error() {
        meos_initialize();
        assert!(std::panic::catch_unwind(|| Quadbin::from_str("not-a-hex-cell")).is_err());
    }
}
