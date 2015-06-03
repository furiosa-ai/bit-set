// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(core)]

/// An implementation of a set using a bit vector as an underlying
/// representation for holding unsigned numerical elements.
///
/// It should also be noted that the amount of storage necessary for holding a
/// set of objects is proportional to the maximum of the objects when viewed
/// as a `usize`.
///
/// # Examples
///
/// ```
/// # #![feature(collections)]
/// use std::collections::{BitSet, BitVec};
///
/// // It's a regular set
/// let mut s = BitSet::new();
/// s.insert(0);
/// s.insert(3);
/// s.insert(7);
///
/// s.remove(&7);
///
/// if !s.contains(&7) {
///     println!("There is no 7");
/// }
///
/// // Can initialize from a `BitVec`
/// let other = BitSet::from_bit_vec(BitVec::from_bytes(&[0b11010000]));
///
/// s.union_with(&other);
///
/// // Print 0, 1, 3 in some order
/// for x in s.iter() {
///     println!("{}", x);
/// }
///
/// // Can convert back to a `BitVec`
/// let bv: BitVec = s.into_bit_vec();
/// assert!(bv[3]);
/// ```



extern crate bit_vec;

use bit_vec::{BitVec, Blocks, BitBlock};
use std::cmp::Ordering;
use std::cmp;
use std::fmt;
use std::hash;
use std::iter::{Chain, Enumerate, Repeat, Skip, Take, repeat};
use std::iter::{self, FromIterator};

type MatchWords<'a, B> = Chain<Enumerate<Blocks<'a, B>>, Skip<Take<Enumerate<Repeat<B>>>>>;

/// Computes how many blocks are needed to store that many bits
fn blocks_for_bits<B: BitBlock>(bits: usize) -> usize {
    // If we want 17 bits, dividing by 32 will produce 0. So we add 1 to make sure we
    // reserve enough. But if we want exactly a multiple of 32, this will actually allocate
    // one too many. So we need to check if that's the case. We can do that by computing if
    // bitwise AND by `32 - 1` is 0. But LLVM should be able to optimize the semantically
    // superior modulo operator on a power of two to this.
    //
    // Note that we can technically avoid this branch with the expression
    // `(nbits + u32::BITS - 1) / 32::BITS`, but if nbits is almost usize::MAX this will overflow.
    if bits % B::bits() == 0 {
        bits / B::bits()
    } else {
        bits / B::bits() + 1
    }
}

// Take two BitVec's, and return iterators of their words, where the shorter one
// has been padded with 0's
fn match_words<'a,'b, B: BitBlock>(a: &'a BitVec<B>, b: &'b BitVec<B>)
		-> (MatchWords<'a, B>, MatchWords<'b, B>) {
    let a_len = a.storage().len();
    let b_len = b.storage().len();

    // have to uselessly pretend to pad the longer one for type matching
    if a_len < b_len {
        (a.blocks().enumerate().chain(iter::repeat(B::zero()).enumerate().take(b_len).skip(a_len)),
         b.blocks().enumerate().chain(iter::repeat(B::zero()).enumerate().take(0).skip(0)))
    } else {
        (a.blocks().enumerate().chain(iter::repeat(B::zero()).enumerate().take(0).skip(0)),
         b.blocks().enumerate().chain(iter::repeat(B::zero()).enumerate().take(a_len).skip(b_len)))
    }
}

pub struct BitSet<B=u32> {
    bit_vec: BitVec<B>,
}

impl<B: BitBlock> Clone for BitSet<B> {
	fn clone(&self) -> Self {
		BitSet {
			bit_vec: self.bit_vec.clone(),
		}
	}
}

impl<B: BitBlock> Default for BitSet<B> {
    #[inline]
    fn default() -> Self { BitSet::new() }
}

impl<B: BitBlock> FromIterator<usize> for BitSet<B> {
    fn from_iter<I: IntoIterator<Item=usize>>(iter: I) -> Self {
        let mut ret = BitSet::new();
        ret.extend(iter);
        ret
    }
}

impl<B: BitBlock> Extend<usize> for BitSet<B> {
    #[inline]
    fn extend<I: IntoIterator<Item=usize>>(&mut self, iter: I) {
        for i in iter {
            self.insert(i);
        }
    }
}

impl<B: BitBlock> PartialOrd for BitSet<B> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::partial_cmp(a_iter, b_iter)
    }
}

impl<B: BitBlock> Ord for BitSet<B> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::cmp(a_iter, b_iter)
    }
}

impl<B: BitBlock> cmp::PartialEq for BitSet<B> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        let (a_iter, b_iter) = match_words(self.get_ref(), other.get_ref());
        iter::order::eq(a_iter, b_iter)
    }
}

impl<B: BitBlock> cmp::Eq for BitSet<B> {}

impl<B: BitBlock> BitSet<B> {
    /// Creates a new empty `BitSet`.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// ```
    #[inline]
    pub fn new() -> Self {
        BitSet { bit_vec: BitVec::new() }
    }

    /// Creates a new `BitSet` with initially no contents, able to
    /// hold `nbits` elements without resizing.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::with_capacity(100);
    /// assert!(s.capacity() >= 100);
    /// ```
    #[inline]
    pub fn with_capacity(nbits: usize) -> Self {
        let bit_vec = BitVec::from_elem(nbits, false);
        BitSet::from_bit_vec(bit_vec)
    }

    /// Creates a new `BitSet` from the given bit vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitVec, BitSet};
    ///
    /// let bv = BitVec::from_bytes(&[0b01100000]);
    /// let s = BitSet::from_bit_vec(bv);
    ///
    /// // Print 1, 2 in arbitrary order
    /// for x in s.iter() {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn from_bit_vec(bit_vec: BitVec<B>) -> Self {
        BitSet { bit_vec: bit_vec }
    }

    /// Returns the capacity in bits for this bit vector. Inserting any
    /// element less than this amount will not trigger a resizing.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::with_capacity(100);
    /// assert!(s.capacity() >= 100);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.bit_vec.capacity()
    }

    /// Reserves capacity for the given `BitSet` to contain `len` distinct elements. In the case
    /// of `BitSet` this means reallocations will not occur as long as all inserted elements
    /// are less than `len`.
    ///
    /// The collection may reserve more space to avoid frequent reallocations.
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// s.reserve_len(10);
    /// assert!(s.capacity() >= 10);
    /// ```
    pub fn reserve_len(&mut self, len: usize) {
        let cur_len = self.bit_vec.len();
        if len >= cur_len {
            self.bit_vec.reserve(len - cur_len);
        }
    }

    /// Reserves the minimum capacity for the given `BitSet` to contain `len` distinct elements.
    /// In the case of `BitSet` this means reallocations will not occur as long as all inserted
    /// elements are less than `len`.
    ///
    /// Note that the allocator may give the collection more space than it requests. Therefore
    /// capacity can not be relied upon to be precisely minimal. Prefer `reserve_len` if future
    /// insertions are expected.
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// s.reserve_len_exact(10);
    /// assert!(s.capacity() >= 10);
    /// ```
    pub fn reserve_len_exact(&mut self, len: usize) {
        let cur_len = self.bit_vec.len();
        if len >= cur_len {
            self.bit_vec.reserve_exact(len - cur_len);
        }
    }


    /// Consumes this set to return the underlying bit vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// s.insert(0);
    /// s.insert(3);
    ///
    /// let bv = s.into_bit_vec();
    /// assert!(bv[0]);
    /// assert!(bv[3]);
    /// ```
    #[inline]
    pub fn into_bit_vec(self) -> BitVec<B> {
        self.bit_vec
    }

    /// Returns a reference to the underlying bit vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// s.insert(0);
    ///
    /// let bv = s.get_ref();
    /// assert_eq!(bv[0], true);
    /// ```
    #[inline]
    pub fn get_ref(&self) -> &BitVec<B> {
        &self.bit_vec
    }

    #[inline]
    fn other_op<F>(&mut self, other: &Self, mut f: F) where F: FnMut(B, B) -> B {
        // Unwrap BitVecs
        let self_bit_vec = &mut self.bit_vec;
        let other_bit_vec = &other.bit_vec;

        let self_len = self_bit_vec.len();
        let other_len = other_bit_vec.len();

        // Expand the vector if necessary
        if self_len < other_len {
            self_bit_vec.grow(other_len - self_len, false);
        }

        // virtually pad other with 0's for equal lengths
        let other_words = {
            let (_, result) = match_words(self_bit_vec, other_bit_vec);
            result
        };

        // Apply values found in other
        for (i, w) in other_words {
            let old = self_bit_vec.storage()[i];
            let new = f(old, w);
            unsafe {
            	self_bit_vec.storage_mut()[i] = new;
            }
        }
    }

    /// Truncates the underlying vector to the least length required.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::BitSet;
    ///
    /// let mut s = BitSet::new();
    /// s.insert(32183231);
    /// s.remove(&32183231);
    ///
    /// // Internal storage will probably be bigger than necessary
    /// println!("old capacity: {}", s.capacity());
    ///
    /// // Now should be smaller
    /// s.shrink_to_fit();
    /// println!("new capacity: {}", s.capacity());
    /// ```
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        let bit_vec = &mut self.bit_vec;
        // Obtain original length
        let old_len = bit_vec.storage().len();
        // Obtain coarse trailing zero length
        let n = bit_vec.storage().iter().rev().take_while(|&&n| n == B::zero()).count();
        // Truncate
        let trunc_len = cmp::max(old_len - n, 1);
        unsafe {
        	bit_vec.storage_mut().truncate(trunc_len);
        	bit_vec.set_len(trunc_len * B::bits());
        }
    }

    /// Iterator over each usize stored in the `BitSet`.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitVec, BitSet};
    ///
    /// let s = BitSet::from_bit_vec(BitVec::from_bytes(&[0b01001010]));
    ///
    /// // Print 1, 4, 6 in arbitrary order
    /// for x in s.iter() {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<B> {
        Iter(BlockIter::from_blocks(self.bit_vec.blocks()))
    }

    /// Iterator over each usize stored in `self` union `other`.
    /// See [union_with](#method.union_with) for an efficient in-place version.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitVec, BitSet};
    ///
    /// let a = BitSet::from_bit_vec(BitVec::from_bytes(&[0b01101000]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[0b10100000]));
    ///
    /// // Print 0, 1, 2, 4 in arbitrary order
    /// for x in a.union(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn union<'a>(&'a self, other: &'a Self) -> Union<'a, B> {
        fn or<B: BitBlock>(w1: B, w2: B) -> B { w1 | w2 }

        Union(BlockIter::from_blocks(TwoBitPositions {
            set: self.bit_vec.blocks(),
            other: other.bit_vec.blocks(),
            merge: or,
        }))
    }

    /// Iterator over each usize stored in `self` intersect `other`.
    /// See [intersect_with](#method.intersect_with) for an efficient in-place version.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitVec, BitSet};
    ///
    /// let a = BitSet::from_bit_vec(BitVec::from_bytes(&[0b01101000]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[0b10100000]));
    ///
    /// // Print 2
    /// for x in a.intersection(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn intersection<'a>(&'a self, other: &'a Self) -> Intersection<'a, B> {
        fn bitand<B: BitBlock>(w1: B, w2: B) -> B { w1 & w2 }
        let min = cmp::min(self.bit_vec.len(), other.bit_vec.len());

        Intersection(BlockIter::from_blocks(TwoBitPositions {
            set: self.bit_vec.blocks(),
            other: other.bit_vec.blocks(),
            merge: bitand,
        }).take(min))
    }

    /// Iterator over each usize stored in the `self` setminus `other`.
    /// See [difference_with](#method.difference_with) for an efficient in-place version.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a = BitSet::from_bit_vec(BitVec::from_bytes(&[0b01101000]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[0b10100000]));
    ///
    /// // Print 1, 4 in arbitrary order
    /// for x in a.difference(&b) {
    ///     println!("{}", x);
    /// }
    ///
    /// // Note that difference is not symmetric,
    /// // and `b - a` means something else.
    /// // This prints 0
    /// for x in b.difference(&a) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn difference<'a>(&'a self, other: &'a Self) -> Difference<'a, B> {
        fn diff<B: BitBlock>(w1: B, w2: B) -> B { w1 & !w2 }

        Difference(BlockIter::from_blocks(TwoBitPositions {
            set: self.bit_vec.blocks(),
            other: other.bit_vec.blocks(),
            merge: diff,
        }))
    }

    /// Iterator over each usize stored in the symmetric difference of `self` and `other`.
    /// See [symmetric_difference_with](#method.symmetric_difference_with) for
    /// an efficient in-place version.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a = BitSet::from_bit_vec(BitVec::from_bytes(&[0b01101000]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[0b10100000]));
    ///
    /// // Print 0, 1, 4 in arbitrary order
    /// for x in a.symmetric_difference(&b) {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn symmetric_difference<'a>(&'a self, other: &'a Self) -> SymmetricDifference<'a, B> {
        fn bitxor<B: BitBlock>(w1: B, w2: B) -> B { w1 ^ w2 }

        SymmetricDifference(BlockIter::from_blocks(TwoBitPositions {
            set: self.bit_vec.blocks(),
            other: other.bit_vec.blocks(),
            merge: bitxor,
        }))
    }

    /// Unions in-place with the specified other bit vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b11101000;
    ///
    /// let mut a = BitSet::from_bit_vec(BitVec::from_bytes(&[a]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[b]));
    /// let res = BitSet::from_bit_vec(BitVec::from_bytes(&[res]));
    ///
    /// a.union_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn union_with(&mut self, other: &Self) {
        self.other_op(other, |w1, w2| w1 | w2);
    }

    /// Intersects in-place with the specified other bit vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b00100000;
    ///
    /// let mut a = BitSet::from_bit_vec(BitVec::from_bytes(&[a]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[b]));
    /// let res = BitSet::from_bit_vec(BitVec::from_bytes(&[res]));
    ///
    /// a.intersect_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn intersect_with(&mut self, other: &Self) {
        self.other_op(other, |w1, w2| w1 & w2);
    }

    /// Makes this bit vector the difference with the specified other bit vector
    /// in-place.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let a_b = 0b01001000; // a - b
    /// let b_a = 0b10000000; // b - a
    ///
    /// let mut bva = BitSet::from_bit_vec(BitVec::from_bytes(&[a]));
    /// let bvb = BitSet::from_bit_vec(BitVec::from_bytes(&[b]));
    /// let bva_b = BitSet::from_bit_vec(BitVec::from_bytes(&[a_b]));
    /// let bvb_a = BitSet::from_bit_vec(BitVec::from_bytes(&[b_a]));
    ///
    /// bva.difference_with(&bvb);
    /// assert_eq!(bva, bva_b);
    ///
    /// let bva = BitSet::from_bit_vec(BitVec::from_bytes(&[a]));
    /// let mut bvb = BitSet::from_bit_vec(BitVec::from_bytes(&[b]));
    ///
    /// bvb.difference_with(&bva);
    /// assert_eq!(bvb, bvb_a);
    /// ```
    #[inline]
    pub fn difference_with(&mut self, other: &Self) {
        self.other_op(other, |w1, w2| w1 & !w2);
    }

    /// Makes this bit vector the symmetric difference with the specified other
    /// bit vector in-place.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections)]
    /// use std::collections::{BitSet, BitVec};
    ///
    /// let a   = 0b01101000;
    /// let b   = 0b10100000;
    /// let res = 0b11001000;
    ///
    /// let mut a = BitSet::from_bit_vec(BitVec::from_bytes(&[a]));
    /// let b = BitSet::from_bit_vec(BitVec::from_bytes(&[b]));
    /// let res = BitSet::from_bit_vec(BitVec::from_bytes(&[res]));
    ///
    /// a.symmetric_difference_with(&b);
    /// assert_eq!(a, res);
    /// ```
    #[inline]
    pub fn symmetric_difference_with(&mut self, other: &Self) {
        self.other_op(other, |w1, w2| w1 ^ w2);
    }

/*
    /// Moves all elements from `other` into `Self`, leaving `other` empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections, bit_set_append_split_off)]
    /// use std::collections::{BitVec, BitSet};
    ///
    /// let mut a = BitSet::new();
    /// a.insert(2);
    /// a.insert(6);
    ///
    /// let mut b = BitSet::new();
    /// b.insert(1);
    /// b.insert(3);
    /// b.insert(6);
    ///
    /// a.append(&mut b);
    ///
    /// assert_eq!(a.len(), 4);
    /// assert_eq!(b.len(), 0);
    /// assert_eq!(a, BitSet::from_bit_vec(BitVec::from_bytes(&[0b01110010])));
    /// ```
    pub fn append(&mut self, other: &mut Self) {
        self.union_with(other);
        other.clear();
    }

    /// Splits the `BitSet` into two at the given key including the key.
    /// Retains the first part in-place while returning the second part.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(collections, bit_set_append_split_off)]
    /// use std::collections::{BitSet, BitVec};
    /// let mut a = BitSet::new();
    /// a.insert(2);
    /// a.insert(6);
    /// a.insert(1);
    /// a.insert(3);
    ///
    /// let b = a.split_off(3);
    ///
    /// assert_eq!(a.len(), 2);
    /// assert_eq!(b.len(), 2);
    /// assert_eq!(a, BitSet::from_bit_vec(BitVec::from_bytes(&[0b01100000])));
    /// assert_eq!(b, BitSet::from_bit_vec(BitVec::from_bytes(&[0b00010010])));
    /// ```
    pub fn split_off(&mut self, at: usize) -> Self {
        let mut other = BitSet::new();

        if at == 0 {
            swap(self, &mut other);
            return other;
        } else if at >= self.bit_vec.len() {
            return other;
        }

        // Calculate block and bit at which to split
        let w = at / u32::BITS;
        let b = at % u32::BITS;

        // Pad `other` with `w` zero blocks,
        // append `self`'s blocks in the range from `w` to the end to `other`
        other.bit_vec.storage_mut().extend(repeat(0u32).take(w)
                                     .chain(self.bit_vec.storage()[w..].iter().cloned()));
        other.bit_vec.nbits = self.bit_vec.nbits;

        if b > 0 {
            other.bit_vec.storage_mut()[w] &= !0 << b;
        }

        // Sets `bit_vec.len()` and fixes the last block as well
        self.bit_vec.truncate(at);

        other
    }
*/

    /// Returns the number of set bits in this set.
    #[inline]
    pub fn len(&self) -> usize  {
        self.bit_vec.blocks().fold(0, |acc, n| acc + n.count_ones() as usize)
    }

    /// Returns whether there are no bits set in this set
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bit_vec.none()
    }

    /// Clears all bits in this set
    #[inline]
    pub fn clear(&mut self) {
        self.bit_vec.clear();
    }

    /// Returns `true` if this set contains the specified integer.
    #[inline]
    pub fn contains(&self, value: &usize) -> bool {
        let bit_vec = &self.bit_vec;
        *value < bit_vec.len() && bit_vec[*value]
    }

    /// Returns `true` if the set has no elements in common with `other`.
    /// This is equivalent to checking for an empty intersection.
    #[inline]
    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.intersection(other).next().is_none()
    }

    /// Returns `true` if the set is a subset of another.
    #[inline]
    pub fn is_subset(&self, other: &Self) -> bool {
        let self_bit_vec = &self.bit_vec;
        let other_bit_vec = &other.bit_vec;
        let other_blocks = blocks_for_bits::<B>(other_bit_vec.len());

        // Check that `self` intersect `other` is self
        self_bit_vec.blocks().zip(other_bit_vec.blocks()).all(|(w1, w2)| w1 & w2 == w1) &&
        // Make sure if `self` has any more blocks than `other`, they're all 0
        self_bit_vec.blocks().skip(other_blocks).all(|w| w == B::zero())
    }

    /// Returns `true` if the set is a superset of another.
    #[inline]
    pub fn is_superset(&self, other: &Self) -> bool {
        other.is_subset(self)
    }

    /// Adds a value to the set. Returns `true` if the value was not already
    /// present in the set.
    pub fn insert(&mut self, value: usize) -> bool {
        if self.contains(&value) {
            return false;
        }

        // Ensure we have enough space to hold the new element
        let len = self.bit_vec.len();
        if value >= len {
            self.bit_vec.grow(value - len + 1, false)
        }

        self.bit_vec.set(value, true);
        return true;
    }

    /// Removes a value from the set. Returns `true` if the value was
    /// present in the set.
    pub fn remove(&mut self, value: &usize) -> bool {
        if !self.contains(value) {
            return false;
        }

        self.bit_vec.set(*value, false);

        return true;
    }
}

impl<B: BitBlock> fmt::Debug for BitSet<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(fmt, "{{"));
        let mut first = true;
        for n in self {
            if !first {
                try!(write!(fmt, ", "));
            }
            try!(write!(fmt, "{:?}", n));
            first = false;
        }
        write!(fmt, "}}")
    }
}

impl<B: BitBlock> hash::Hash for BitSet<B> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        for pos in self {
            pos.hash(state);
        }
    }
}

#[derive(Clone)]
struct BlockIter<T, B> {
    head: B,
    head_offset: usize,
    tail: T,
}

impl<T, B: BitBlock> BlockIter<T, B> where T: Iterator<Item=B> {
    fn from_blocks(mut blocks: T) -> BlockIter<T, B> {
        let h = blocks.next().unwrap_or(B::zero());
        BlockIter {tail: blocks, head: h, head_offset: 0}
    }
}

/// An iterator combining two `BitSet` iterators.
#[derive(Clone)]
struct TwoBitPositions<'a, B: 'a> {
    set: Blocks<'a, B>,
    other: Blocks<'a, B>,
    merge: fn(B, B) -> B,
}

/// An iterator for `BitSet`.
#[derive(Clone)]
pub struct Iter<'a, B: 'a>(BlockIter<Blocks<'a, B>, B>);
#[derive(Clone)]
pub struct Union<'a, B: 'a>(BlockIter<TwoBitPositions<'a, B>, B>);
#[derive(Clone)]
pub struct Intersection<'a, B: 'a>(Take<BlockIter<TwoBitPositions<'a, B>, B>>);
#[derive(Clone)]
pub struct Difference<'a, B: 'a>(BlockIter<TwoBitPositions<'a, B>, B>);
#[derive(Clone)]
pub struct SymmetricDifference<'a, B: 'a>(BlockIter<TwoBitPositions<'a, B>, B>);

impl<'a, T, B: BitBlock> Iterator for BlockIter<T, B> where T: Iterator<Item=B> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.head == B::zero() {
            match self.tail.next() {
                Some(w) => self.head = w,
                None => return None
            }
            self.head_offset += B::bits();
        }

        // from the current block, isolate the
        // LSB and subtract 1, producing k:
        // a block with a number of set bits
        // equal to the index of the LSB
        let k = (self.head & (!self.head + B::one())) - B::one();
        // update block, removing the LSB
        self.head = self.head & (self.head - B::one());
        // return offset + (index of LSB)
        Some(self.head_offset + (B::count_ones(k) as usize))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.tail.size_hint() {
            (_, Some(h)) => (0, Some(1 + h * B::bits())),
            _ => (0, None)
        }
    }
}

impl<'a, B: BitBlock> Iterator for TwoBitPositions<'a, B> {
    type Item = B;

    fn next(&mut self) -> Option<B> {
        match (self.set.next(), self.other.next()) {
            (Some(a), Some(b)) => Some((self.merge)(a, b)),
            (Some(a), None) => Some((self.merge)(a, B::zero())),
            (None, Some(b)) => Some((self.merge)(B::zero(), b)),
            _ => return None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (a, au) = self.set.size_hint();
        let (b, bu) = self.other.size_hint();

        let upper = match (au, bu) {
            (Some(au), Some(bu)) => Some(cmp::max(au, bu)),
            _ => None
        };

        (cmp::max(a, b), upper)
    }
}

impl<'a, B: BitBlock> Iterator for Iter<'a, B> {
    type Item = usize;

    #[inline] fn next(&mut self) -> Option<usize> { self.0.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}

impl<'a, B: BitBlock> Iterator for Union<'a, B> {
    type Item = usize;

    #[inline] fn next(&mut self) -> Option<usize> { self.0.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}

impl<'a, B: BitBlock> Iterator for Intersection<'a, B> {
    type Item = usize;

    #[inline] fn next(&mut self) -> Option<usize> { self.0.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}

impl<'a, B: BitBlock> Iterator for Difference<'a, B> {
    type Item = usize;

    #[inline] fn next(&mut self) -> Option<usize> { self.0.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}

impl<'a, B: BitBlock> Iterator for SymmetricDifference<'a, B> {
    type Item = usize;

    #[inline] fn next(&mut self) -> Option<usize> { self.0.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}

impl<'a, B: BitBlock> IntoIterator for &'a BitSet<B> {
    type Item = usize;
    type IntoIter = Iter<'a, B>;

    fn into_iter(self) -> Iter<'a, B> {
        self.iter()
    }
}