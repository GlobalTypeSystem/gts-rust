//! Type evidence used by the macro when an earlier ID alias uses normal Serde serialization.
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};

#[diagnostic::on_unimplemented(
    message = "JSON field of type `{Self}` can shadow a lower-priority GTS instance ID"
)]
pub trait NonStringIdField {}

// The trait's module is private. Callers can use the check through generated code,
// but cannot make a string-producing type pass it.
pub const fn check<T: ?Sized + NonStringIdField>() {}

macro_rules! scalars {
    ($($ty:ty),*) => { $(impl NonStringIdField for $ty {})* };
}
scalars!(
    bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

impl<T> NonStringIdField for [T] {}
impl<T, const N: usize> NonStringIdField for [T; N] {}
impl<T> NonStringIdField for Vec<T> {}
impl<T> NonStringIdField for VecDeque<T> {}
#[expect(
    clippy::disallowed_types,
    reason = "recognizes the wire shape of a caller's list without allocating one"
)]
impl<T> NonStringIdField for std::collections::LinkedList<T> {}
impl<T> NonStringIdField for BinaryHeap<T> {}
impl<T> NonStringIdField for BTreeSet<T> {}
impl<K, V> NonStringIdField for BTreeMap<K, V> {}
impl<T, S> NonStringIdField for HashSet<T, S> {}
impl<K, V, S> NonStringIdField for HashMap<K, V, S> {}
impl<T: NonStringIdField> NonStringIdField for Option<T> {}
impl<T: ?Sized + NonStringIdField> NonStringIdField for &T {}
impl<T: ?Sized + NonStringIdField> NonStringIdField for &mut T {}
impl<T: ?Sized + NonStringIdField> NonStringIdField for Box<T> {}

// Serde supports tuples up to arity 16. Their members' shapes do not affect
// the tuple's JSON representation.
macro_rules! tuples {
    () => { impl NonStringIdField for () {} };
    ($first:ident $(, $rest:ident)*) => {
        impl<$first, $($rest),*> NonStringIdField for ($first, $($rest,)*) {}
        tuples!($($rest),*);
    };
}
tuples!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
