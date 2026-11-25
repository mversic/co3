//! Language contracts used by conversion traits; these do not change representation.
//!
//! The contract applies to the mapped native source type: canonical booleans,
//! Swift `Unicode.Scalar` for Rust `char`, widened Java/C# UTF-16 code units,
//! and Swift string UTF-8 views. Python character values retain surrogate checks.
//! C# booleans require one-byte marshalling. Arbitrary carrier construction does
//! not establish these guarantees. Unknown languages retain checked behavior.

/// Supplies validation under the mapped native source contract.
///
/// Defaults perform full validation. Overrides may rely on guarantees that
/// callers of language-aware conversions must uphold.
pub trait Language {
    /// Validate the corresponding mapped value under this language contract.
    #[inline(always)]
    fn validate_bool(value: u8) -> bool {
        value <= 1
    }

    /// Validate the corresponding mapped value under this language contract.
    #[inline(always)]
    fn validate_char(value: u32) -> bool {
        char::from_u32(value).is_some()
    }

    /// Validate the corresponding mapped value under this language contract.
    #[inline(always)]
    fn validate_ordering(value: i8) -> bool {
        (-1..=1).contains(&value)
    }

    /// Validate the corresponding mapped value under this language contract.
    #[inline(always)]
    fn validate_nonzero(is_zero: bool) -> bool {
        !is_zero
    }

    /// Validate the corresponding mapped value under this language contract.
    #[inline(always)]
    fn validate_utf8(value: &[u8]) -> bool {
        core::str::from_utf8(value).is_ok()
    }
}

/// C mapped native value contract.
pub enum C {}

/// Cpp mapped native value contract.
pub enum Cpp {}

/// Swift mapped native value contract.
pub enum Swift {}

/// ObjectiveC mapped native value contract.
pub enum ObjectiveC {}

/// Java mapped native value contract.
pub enum Java {}

/// CSharp mapped native value contract.
pub enum CSharp {}

/// Python mapped native value contract.
pub enum Python {}

impl Language for C {}

impl Language for Cpp {
    #[inline(always)]
    fn validate_bool(_value: u8) -> bool {
        true
    }
}

impl Language for Swift {
    #[inline(always)]
    fn validate_bool(_value: u8) -> bool {
        true
    }
    #[inline(always)]
    fn validate_char(_value: u32) -> bool {
        true
    }
    #[inline(always)]
    fn validate_utf8(_value: &[u8]) -> bool {
        true
    }
}

impl Language for ObjectiveC {}

impl Language for Java {
    #[inline(always)]
    fn validate_bool(_value: u8) -> bool {
        true
    }
    #[inline(always)]
    fn validate_char(value: u32) -> bool {
        !(0xd800..=0xdfff).contains(&value)
    }
}

impl Language for CSharp {
    #[inline(always)]
    fn validate_bool(_value: u8) -> bool {
        true
    }
    #[inline(always)]
    fn validate_char(value: u32) -> bool {
        !(0xd800..=0xdfff).contains(&value)
    }
}

impl Language for Python {
    #[inline(always)]
    fn validate_bool(_value: u8) -> bool {
        true
    }
    #[inline(always)]
    fn validate_char(value: u32) -> bool {
        !(0xd800..=0xdfff).contains(&value)
    }
}

pub(crate) mod checks {
    //! Proofs for individual source-type validity checks.
    //!
    //! These do not bypass structural conversion, custom predicates, or stores.

    use super::Language;

    #[inline(always)]
    pub(crate) fn boolean<L: Language>(value: u8) -> bool {
        L::validate_bool(value)
    }

    #[inline(always)]
    pub(crate) fn unicode_scalar<L: Language>(value: u32) -> bool {
        L::validate_char(value)
    }

    #[inline(always)]
    pub(crate) fn ordering<L: Language>(value: i8) -> bool {
        L::validate_ordering(value)
    }

    #[inline(always)]
    pub(crate) fn nonzero<L: Language>(is_zero: bool) -> bool {
        L::validate_nonzero(is_zero)
    }

    #[inline(always)]
    pub(crate) fn utf8<L: Language>(value: &[u8]) -> bool {
        L::validate_utf8(value)
    }

    /// The caller must uphold the selected language's UTF-8 source contract.
    #[cfg(feature = "alloc")]
    #[inline(always)]
    pub(crate) unsafe fn decode_utf8<L: Language>(value: &[u8]) -> Option<&str> {
        if utf8::<L>(value) {
            Some(unsafe { core::str::from_utf8_unchecked(value) })
        } else {
            None
        }
    }
}
