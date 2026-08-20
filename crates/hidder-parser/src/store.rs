//! Pluggable record buffers for semantic parse output.
//!
//! The default store is [`VecStore`]. Other standard `alloc` containers work
//! through [`LinkedListStore`] and [`VecDequeStore`], and user types can
//! implement [`RecordBuf`] / [`DescriptorStore`] (for example a bounded
//! `heapless::Vec`).

use alloc::{
    collections::{
        LinkedList,
        VecDeque,
    },
    vec::Vec,
};
use core::{
    convert::Infallible,
    fmt,
};

use crate::{
    Collection,
    Diagnostic,
    LongItemRecord,
    ReportField,
    ReportLayout,
    UnknownItemRecord,
};

/// A growable buffer that can hold parse records.
///
/// `Vec<T>` is implemented out of the box. Implement this trait for a custom
/// container (bounded vector, arena list, …) to store parse results without
/// `alloc::vec::Vec`.
pub trait RecordBuf<T>: Default {
    /// Failure from [`RecordBuf::try_push`], such as a capacity limit.
    type Error;

    /// Appends `value`, or returns a store-specific error.
    ///
    /// # Errors
    ///
    /// Returns [`RecordBuf::Error`] when the buffer cannot accept another
    /// record.
    fn try_push(&mut self, value: T) -> Result<(), Self::Error>;

    /// Number of stored records.
    fn len(&self) -> usize;

    /// Returns whether the buffer contains no records.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows the record at `index`.
    fn get(&self, index: usize) -> Option<&T>;

    /// Mutably borrows the record at `index`.
    fn get_mut(&mut self, index: usize) -> Option<&mut T>;

    /// Iterates records in insertion order.
    fn iter<'b>(&'b self) -> impl Iterator<Item = &'b T> + 'b
    where
        T: 'b;

    /// Mutably iterates records in insertion order.
    fn iter_mut<'b>(&'b mut self) -> impl Iterator<Item = &'b mut T> + 'b
    where
        T: 'b;
}

impl<T> RecordBuf<T> for Vec<T> {
    type Error = Infallible;

    fn try_push(&mut self, value: T) -> Result<(), Self::Error> {
        self.push(value);
        Ok(())
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn get(&self, index: usize) -> Option<&T> {
        AsRef::<[T]>::as_ref(self).get(index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        AsMut::<[T]>::as_mut(self).get_mut(index)
    }

    fn iter<'b>(&'b self) -> impl Iterator<Item = &'b T> + 'b
    where
        T: 'b,
    {
        <[T]>::iter(self)
    }

    fn iter_mut<'b>(&'b mut self) -> impl Iterator<Item = &'b mut T> + 'b
    where
        T: 'b,
    {
        <[T]>::iter_mut(self)
    }
}

impl<T> RecordBuf<T> for VecDeque<T> {
    type Error = Infallible;

    fn try_push(&mut self, value: T) -> Result<(), Self::Error> {
        self.push_back(value);
        Ok(())
    }

    fn len(&self) -> usize {
        VecDeque::len(self)
    }

    fn get(&self, index: usize) -> Option<&T> {
        VecDeque::get(self, index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        VecDeque::get_mut(self, index)
    }

    fn iter<'b>(&'b self) -> impl Iterator<Item = &'b T> + 'b
    where
        T: 'b,
    {
        VecDeque::iter(self)
    }

    fn iter_mut<'b>(&'b mut self) -> impl Iterator<Item = &'b mut T> + 'b
    where
        T: 'b,
    {
        VecDeque::iter_mut(self)
    }
}

impl<T> RecordBuf<T> for LinkedList<T> {
    type Error = Infallible;

    fn try_push(&mut self, value: T) -> Result<(), Self::Error> {
        self.push_back(value);
        Ok(())
    }

    fn len(&self) -> usize {
        LinkedList::len(self)
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.iter().nth(index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.iter_mut().nth(index)
    }

    fn iter<'b>(&'b self) -> impl Iterator<Item = &'b T> + 'b
    where
        T: 'b,
    {
        LinkedList::iter(self)
    }

    fn iter_mut<'b>(&'b mut self) -> impl Iterator<Item = &'b mut T> + 'b
    where
        T: 'b,
    {
        LinkedList::iter_mut(self)
    }
}

/// Family of buffers used to store one complete semantic parse.
pub trait DescriptorStore<'a> {
    /// Shared push error for every buffer in this store.
    type Error: fmt::Debug + Eq + PartialEq;
    /// Input/Output/Feature fields.
    type Fields: RecordBuf<ReportField, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
    /// Collection tree.
    type Collections: RecordBuf<Collection, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
    /// Per-kind, per-ID layouts.
    type Reports: RecordBuf<ReportLayout, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
    /// Borrowed long items.
    type LongItems: RecordBuf<LongItemRecord<'a>, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
    /// Borrowed unknown short items.
    type UnknownItems: RecordBuf<UnknownItemRecord<'a>, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
    /// Parser diagnostics.
    type Diagnostics: RecordBuf<Diagnostic, Error = Self::Error> + Clone + fmt::Debug + Eq + PartialEq;
}

/// [`alloc::vec::Vec`] buffers. This is the default store for [`crate::parse`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VecStore;

impl<'a> DescriptorStore<'a> for VecStore {
    type Collections = Vec<Collection>;
    type Diagnostics = Vec<Diagnostic>;
    type Error = Infallible;
    type Fields = Vec<ReportField>;
    type LongItems = Vec<LongItemRecord<'a>>;
    type Reports = Vec<ReportLayout>;
    type UnknownItems = Vec<UnknownItemRecord<'a>>;
}

/// [`alloc::collections::VecDeque`] buffers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VecDequeStore;

impl<'a> DescriptorStore<'a> for VecDequeStore {
    type Collections = VecDeque<Collection>;
    type Diagnostics = VecDeque<Diagnostic>;
    type Error = Infallible;
    type Fields = VecDeque<ReportField>;
    type LongItems = VecDeque<LongItemRecord<'a>>;
    type Reports = VecDeque<ReportLayout>;
    type UnknownItems = VecDeque<UnknownItemRecord<'a>>;
}

/// [`alloc::collections::LinkedList`] buffers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinkedListStore;

impl<'a> DescriptorStore<'a> for LinkedListStore {
    type Collections = LinkedList<Collection>;
    type Diagnostics = LinkedList<Diagnostic>;
    type Error = Infallible;
    type Fields = LinkedList<ReportField>;
    type LongItems = LinkedList<LongItemRecord<'a>>;
    type Reports = LinkedList<ReportLayout>;
    type UnknownItems = LinkedList<UnknownItemRecord<'a>>;
}

/// Failure from [`crate::parse_in`] / [`crate::parse_in_with_options`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseFailure<D, E> {
    /// The descriptor contained at least one error-level diagnostic.
    Invalid(D),
    /// A record buffer rejected an append.
    Store(E),
}

impl<D: fmt::Debug, E: fmt::Debug> fmt::Display for ParseFailure<D, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Invalid(diagnostics) => {
                write!(f, "Invalid HID report descriptor: {diagnostics:?}")
            },
            | Self::Store(error) => {
                write!(f, "HID report descriptor store rejected a record: {error:?}")
            },
        }
    }
}

impl<D: fmt::Debug, E: fmt::Debug> core::error::Error for ParseFailure<D, E> {}
