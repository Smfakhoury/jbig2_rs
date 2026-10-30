//! Codebook implementation.

#[cfg(verus_keep_ghost)]
use super::*;
#[cfg(not(verus_keep_ghost))]
use super::{Bitmap, MalformedError, SymbolCodebook, SymbolTextError};

impl<'a> SymbolCodebook<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        ensures out as nat == symbol_text_spec::codebook_len(self),
    ))]
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Contiguous(symbols) => symbols.len(),
            Self::Borrowed(symbols) => symbols.len(),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(out =>
        requires symbol_text_spec::codebook_valid(self),
        ensures match out {
            Some(bitmap) => crate::spec::bitmap::storage_valid(bitmap)
                && symbol_text_spec::codebook_selects(self, index as nat, bitmap),
            None => index as nat >= symbol_text_spec::codebook_len(self),
        },
    ))]
    pub(crate) fn get(self, index: usize) -> Option<&'a Bitmap> {
        match self {
            Self::Contiguous(symbols) => symbols.get(index),
            Self::Borrowed(symbols) => match symbols.get(index) {
                Some(bitmap) => Some(*bitmap),
                None => None,
            },
        }
    }
}

/// Looks up a symbol by ID across the imported and already-decoded-new
/// symbol collections, as used by T.88 6.5.8.2.2's single-instance
/// refinement/aggregate procedure.
#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        symbol_text_spec::codebook_valid(imported),
        symbol_text_spec::bitmaps_valid(new_symbols@),
    ensures match out {
        Ok(bitmap) => crate::spec::bitmap::storage_valid(bitmap)
            && symbol_text_spec::symbol_id_selects(imported, new_symbols@, id, bitmap),
        Err(_) => true,
    },
    (symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= usize::MAX
        && symbol_text_spec::codebook_len(imported) + new_symbols@.len() <= u32::MAX
        && (id as nat) < symbol_text_spec::codebook_len(imported) + new_symbols@.len())
        ==> out.is_ok(),
))]
pub(super) fn symbol_by_id<'a>(
    imported: SymbolCodebook<'a>,
    new_symbols: &'a [Bitmap],
    id: u32,
) -> Result<&'a Bitmap, SymbolTextError> {
    let id_usize = usize::try_from(id).map_err(|_error| MalformedError::IntegerOverflow)?;
    let available = imported
        .len()
        .checked_add(new_symbols.len())
        .ok_or(MalformedError::IntegerOverflow)?;
    let available_u32 =
        u32::try_from(available).map_err(|_error| MalformedError::IntegerOverflow)?;
    if id_usize < imported.len() {
        imported
            .get(id_usize)
            .ok_or(SymbolTextError::SymbolIdOutOfRange {
                id,
                available: available_u32,
            })
    } else {
        let new_index = id_usize - imported.len();
        if new_index < new_symbols.len() {
            Ok(&new_symbols[new_index])
        } else {
            Err(SymbolTextError::SymbolIdOutOfRange {
                id,
                available: available_u32,
            })
        }
    }
}
