// Copyright 2015 The Rust Project Developers. See the COPYRIGHT file at the
// top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Thread-local reference-counted boxes (the `Cc<T>` type).
//!
//! The `Cc<T>` type provides shared ownership of an immutable value.
//! Destruction is deterministic, and will occur as soon as the last owner is
//! gone. It is marked as non-sendable because it avoids the overhead of atomic
//! reference counting.
//!
//! The `downgrade` method can be used to create a non-owning `Weak<T>` pointer
//! to the box. A `Weak<T>` pointer can be upgraded to an `Cc<T>` pointer, but
//! will return `None` if the value has already been dropped.
//!
//! For example, a tree with parent pointers can be represented by putting the
//! nodes behind strong `Cc<T>` pointers, and then storing the parent pointers
//! as `Weak<T>` pointers.
//!
//! # Examples
//!
//! Consider a scenario where a set of `Gadget`s are owned by a given `Owner`.
//! We want to have our `Gadget`s point to their `Owner`. We can't do this with
//! unique ownership, because more than one gadget may belong to the same
//! `Owner`. `Cc<T>` allows us to share an `Owner` between multiple `Gadget`s,
//! and have the `Owner` remain allocated as long as any `Gadget` points at it.
//!
//! ```rust
//! # #![feature(alloc, collections)]
//! use bacon_rajan_cc::Cc;
//!
//! struct Owner {
//!     name: String
//!     // ...other fields
//! }
//!
//! struct Gadget {
//!     id: i32,
//!     owner: Cc<Owner>
//!     // ...other fields
//! }
//!
//! fn main() {
//!     // Create a reference counted Owner.
//!     let gadget_owner : Cc<Owner> = Cc::new(
//!             Owner { name: String::from_str("Gadget Man") }
//!     );
//!
//!     // Create Gadgets belonging to gadget_owner. To increment the reference
//!     // count we clone the `Cc<T>` object.
//!     let gadget1 = Gadget { id: 1, owner: gadget_owner.clone() };
//!     let gadget2 = Gadget { id: 2, owner: gadget_owner.clone() };
//!
//!     drop(gadget_owner);
//!
//!     // Despite dropping gadget_owner, we're still able to print out the name
//!     // of the Owner of the Gadgets. This is because we've only dropped the
//!     // reference count object, not the Owner it wraps. As long as there are
//!     // other `Cc<T>` objects pointing at the same Owner, it will remain
//!     // allocated. Notice that the `Cc<T>` wrapper around Gadget.owner gets
//!     // automatically dereferenced for us.
//!     println!("Gadget {} owned by {}", gadget1.id, gadget1.owner.name);
//!     println!("Gadget {} owned by {}", gadget2.id, gadget2.owner.name);
//!
//!     // At the end of the method, gadget1 and gadget2 get destroyed, and with
//!     // them the last counted references to our Owner. Gadget Man now gets
//!     // destroyed as well.
//! }
//! ```
//!
//! If our requirements change, and we also need to be able to traverse from
//! Owner → Gadget, we will run into problems: an `Cc<T>` pointer from Owner
//! → Gadget introduces a cycle between the objects. This means that their
//! reference counts can never reach 0, and the objects will remain allocated: a
//! memory leak. In order to get around this, we can use `Weak<T>` pointers.
//! These pointers don't contribute to the total count.
//!
//! Rust actually makes it somewhat difficult to produce this loop in the first
//! place: in order to end up with two objects that point at each other, one of
//! them needs to be mutable. This is problematic because `Cc<T>` enforces
//! memory safety by only giving out shared references to the object it wraps,
//! and these don't allow direct mutation. We need to wrap the part of the
//! object we wish to mutate in a `RefCell`, which provides *interior
//! mutability*: a method to achieve mutability through a shared reference.
//! `RefCell` enforces Rust's borrowing rules at runtime.  Read the `Cell`
//! documentation for more details on interior mutability.
//!
//! ```rust
//! # #![feature(alloc)]
//! use bacon_rajan_cc::Cc;
//! use bacon_rajan_cc::Weak;
//! use std::cell::RefCell;
//!
//! struct Owner {
//!     name: String,
//!     gadgets: RefCell<Vec<Weak<Gadget>>>
//!     // ...other fields
//! }
//!
//! struct Gadget {
//!     id: i32,
//!     owner: Cc<Owner>
//!     // ...other fields
//! }
//!
//! fn main() {
//!     // Create a reference counted Owner. Note the fact that we've put the
//!     // Owner's vector of Gadgets inside a RefCell so that we can mutate it
//!     // through a shared reference.
//!     let gadget_owner : Cc<Owner> = Cc::new(
//!             Owner {
//!                 name: "Gadget Man".to_string(),
//!                 gadgets: RefCell::new(Vec::new())
//!             }
//!     );
//!
//!     // Create Gadgets belonging to gadget_owner as before.
//!     let gadget1 = Cc::new(Gadget{id: 1, owner: gadget_owner.clone()});
//!     let gadget2 = Cc::new(Gadget{id: 2, owner: gadget_owner.clone()});
//!
//!     // Add the Gadgets to their Owner. To do this we mutably borrow from
//!     // the RefCell holding the Owner's Gadgets.
//!     gadget_owner.gadgets.borrow_mut().push(gadget1.clone().downgrade());
//!     gadget_owner.gadgets.borrow_mut().push(gadget2.clone().downgrade());
//!
//!     // Iterate over our Gadgets, printing their details out
//!     for gadget_opt in gadget_owner.gadgets.borrow().iter() {
//!
//!         // gadget_opt is a Weak<Gadget>. Since weak pointers can't guarantee
//!         // that their object is still allocated, we need to call upgrade()
//!         // on them to turn them into a strong reference. This returns an
//!         // Option, which contains a reference to our object if it still
//!         // exists.
//!         let gadget = gadget_opt.upgrade().unwrap();
//!         println!("Gadget {} owned by {}", gadget.id, gadget.owner.name);
//!     }
//!
//!     // At the end of the method, gadget_owner, gadget1 and gadget2 get
//!     // destroyed. There are now no strong (`Cc<T>`) references to the gadgets.
//!     // Once they get destroyed, the Gadgets get destroyed. This zeroes the
//!     // reference count on Gadget Man, so he gets destroyed as well.
//! }
//! ```

#![feature(alloc)]
#![feature(core)]
#![feature(custom_derive)]
#![feature(filling_drop)]
#![feature(plugin)]
#![feature(plugin_registrar)]
#![feature(quote)]
#![feature(rustc_private)]
#![feature(trace_macros)]
#![feature(unsafe_no_drop_flag)]

#[macro_use]
extern crate syntax;
#[macro_use]
extern crate rustc;

use std::boxed;

extern crate core;
use core::cell::Cell;
use core::clone::Clone;
use core::cmp::{PartialEq, PartialOrd, Eq, Ord, Ordering};
use core::default::Default;
use core::fmt;
use core::hash::{Hasher, Hash};
use core::mem::{self, min_align_of, size_of, forget};
use core::nonzero::NonZero;
use core::ops::{Deref, Drop};
use core::option::Option;
use core::option::Option::{Some, None};
use core::ptr;
use core::result::Result;
use core::result::Result::{Ok, Err};
use core::intrinsics::assume;

extern crate alloc;
use alloc::heap::deallocate;

/// TODO FITZGEN
pub mod trace_plugin;
pub use trace_plugin::*;

struct CcBox<T> {
    value: T,
    strong: Cell<usize>,
    weak: Cell<usize>
}

/// A reference-counted pointer type over an immutable value.
///
/// See the [module level documentation](./) for more details.
#[unsafe_no_drop_flag]
pub struct Cc<T> {
    // FIXME #12808: strange names to try to avoid interfering with field
    // accesses of the contained type via Deref
    _ptr: NonZero<*mut CcBox<T>>,
}

impl<T> Cc<T> {
    /// Constructs a new `Cc<T>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    /// ```
    pub fn new(value: T) -> Cc<T> {
        unsafe {
            Cc {
                // there is an implicit weak pointer owned by all the strong
                // pointers, which ensures that the weak destructor never frees
                // the allocation while the strong destructor is running, even
                // if the weak pointer is stored inside the strong one.
                _ptr: NonZero::new(boxed::into_raw(Box::new(CcBox {
                    value: value,
                    strong: Cell::new(1),
                    weak: Cell::new(1)
                }))),
            }
        }
    }

    /// Downgrades the `Cc<T>` to a `Weak<T>` reference.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// let weak_five = five.downgrade();
    /// ```
    pub fn downgrade(&self) -> Weak<T> {
        self.inc_weak();
        Weak { _ptr: self._ptr }
    }
}

/// Get the number of weak references to this value.
#[inline]
pub fn weak_count<T>(this: &Cc<T>) -> usize { this.weak() - 1 }

/// Get the number of strong references to this value.
#[inline]
pub fn strong_count<T>(this: &Cc<T>) -> usize { this.strong() }

/// Returns true if there are no other `Cc` or `Weak<T>` values that share the
/// same inner value.
///
/// # Examples
///
/// ```
/// # #![feature(alloc)]
/// use bacon_rajan_cc;
/// use bacon_rajan_cc::Cc;
///
/// let five = Cc::new(5);
///
/// bacon_rajan_cc::is_unique(&five);
/// ```
#[inline]
pub fn is_unique<T>(rc: &Cc<T>) -> bool {
    weak_count(rc) == 0 && strong_count(rc) == 1
}

/// Unwraps the contained value if the `Cc<T>` is unique.
///
/// If the `Cc<T>` is not unique, an `Err` is returned with the same `Cc<T>`.
///
/// # Examples
///
/// ```
/// # #![feature(alloc)]
/// use bacon_rajan_cc::{self, Cc};
///
/// let x = Cc::new(3);
/// assert_eq!(bacon_rajan_cc::try_unwrap(x), Ok(3));
///
/// let x = Cc::new(4);
/// let _y = x.clone();
/// assert_eq!(bacon_rajan_cc::try_unwrap(x), Err(Cc::new(4)));
/// ```
#[inline]
pub fn try_unwrap<T>(rc: Cc<T>) -> Result<T, Cc<T>> {
    if is_unique(&rc) {
        unsafe {
            let val = ptr::read(&*rc); // copy the contained object
            // destruct the box and skip our Drop
            // we can ignore the refcounts because we know we're unique
            deallocate(*rc._ptr as *mut u8, size_of::<CcBox<T>>(),
                        min_align_of::<CcBox<T>>());
            forget(rc);
            Ok(val)
        }
    } else {
        Err(rc)
    }
}

/// Returns a mutable reference to the contained value if the `Cc<T>` is unique.
///
/// Returns `None` if the `Cc<T>` is not unique.
///
/// # Examples
///
/// ```
/// # #![feature(alloc)]
/// use bacon_rajan_cc::{self, Cc};
///
/// let mut x = Cc::new(3);
/// *bacon_rajan_cc::get_mut(&mut x).unwrap() = 4;
/// assert_eq!(*x, 4);
///
/// let _y = x.clone();
/// assert!(bacon_rajan_cc::get_mut(&mut x).is_none());
/// ```
#[inline]
pub fn get_mut<T>(rc: &mut Cc<T>) -> Option<&mut T> {
    if is_unique(rc) {
        let inner = unsafe { &mut **rc._ptr };
        Some(&mut inner.value)
    } else {
        None
    }
}

impl<T: Clone> Cc<T> {
    /// Make a mutable reference from the given `Cc<T>`.
    ///
    /// This is also referred to as a copy-on-write operation because the inner
    /// data is cloned if the reference count is greater than one.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// let mut five = Cc::new(5);
    ///
    /// let mut_five = five.make_unique();
    /// ```
    #[inline]
    pub fn make_unique(&mut self) -> &mut T {
        if !is_unique(self) {
            *self = Cc::new((**self).clone())
        }
        // This unsafety is ok because we're guaranteed that the pointer
        // returned is the *only* pointer that will ever be returned to T. Our
        // reference count is guaranteed to be 1 at this point, and we required
        // the `Cc<T>` itself to be `mut`, so we're returning the only possible
        // reference to the inner value.
        let inner = unsafe { &mut **self._ptr };
        &mut inner.value
    }
}

impl<T> Deref for Cc<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.inner().value
    }
}

impl<T> Drop for Cc<T> {
    /// Drops the `Cc<T>`.
    ///
    /// This will decrement the strong reference count. If the strong reference
    /// count becomes zero and the only other references are `Weak<T>` ones,
    /// `drop`s the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// {
    ///     let five = Cc::new(5);
    ///
    ///     // stuff
    ///
    ///     drop(five); // explicit drop
    /// }
    /// {
    ///     let five = Cc::new(5);
    ///
    ///     // stuff
    ///
    /// } // implicit drop
    /// ```
    fn drop(&mut self) {
        unsafe {
            let ptr = *self._ptr;
            if !ptr.is_null() && ptr as usize != mem::POST_DROP_USIZE {
                self.dec_strong();
                if self.strong() == 0 {
                    ptr::read(&**self); // destroy the contained object

                    // remove the implicit "strong weak" pointer now that we've
                    // destroyed the contents.
                    self.dec_weak();

                    if self.weak() == 0 {
                        deallocate(ptr as *mut u8, size_of::<CcBox<T>>(),
                                   min_align_of::<CcBox<T>>())
                    }
                }
            }
        }
    }
}

impl<T> Clone for Cc<T> {

    /// Makes a clone of the `Cc<T>`.
    ///
    /// When you clone an `Cc<T>`, it will create another pointer to the data and
    /// increase the strong reference counter.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five.clone();
    /// ```
    #[inline]
    fn clone(&self) -> Cc<T> {
        self.inc_strong();
        Cc { _ptr: self._ptr }
    }
}

impl<T: Default> Default for Cc<T> {
    /// Creates a new `Cc<T>`, with the `Default` value for `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let x: Cc<i32> = Default::default();
    /// ```
    #[inline]
    fn default() -> Cc<T> {
        Cc::new(Default::default())
    }
}

impl<T: PartialEq> PartialEq for Cc<T> {
    /// Equality for two `Cc<T>`s.
    ///
    /// Two `Cc<T>`s are equal if their inner value are equal.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five == Cc::new(5);
    /// ```
    #[inline(always)]
    fn eq(&self, other: &Cc<T>) -> bool { **self == **other }

    /// Inequality for two `Cc<T>`s.
    ///
    /// Two `Cc<T>`s are unequal if their inner value are unequal.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five != Cc::new(5);
    /// ```
    #[inline(always)]
    fn ne(&self, other: &Cc<T>) -> bool { **self != **other }
}

impl<T: Eq> Eq for Cc<T> {}

impl<T: PartialOrd> PartialOrd for Cc<T> {
    /// Partial comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `partial_cmp()` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five.partial_cmp(&Cc::new(5));
    /// ```
    #[inline(always)]
    fn partial_cmp(&self, other: &Cc<T>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }

    /// Less-than comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `<` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five < Cc::new(5);
    /// ```
    #[inline(always)]
    fn lt(&self, other: &Cc<T>) -> bool { **self < **other }

    /// 'Less-than or equal to' comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `<=` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five <= Cc::new(5);
    /// ```
    #[inline(always)]
    fn le(&self, other: &Cc<T>) -> bool { **self <= **other }

    /// Greater-than comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `>` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five > Cc::new(5);
    /// ```
    #[inline(always)]
    fn gt(&self, other: &Cc<T>) -> bool { **self > **other }

    /// 'Greater-than or equal to' comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `>=` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five >= Cc::new(5);
    /// ```
    #[inline(always)]
    fn ge(&self, other: &Cc<T>) -> bool { **self >= **other }
}

impl<T: Ord> Ord for Cc<T> {
    /// Comparison for two `Cc<T>`s.
    ///
    /// The two are compared by calling `cmp()` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// five.partial_cmp(&Cc::new(5));
    /// ```
    #[inline]
    fn cmp(&self, other: &Cc<T>) -> Ordering { (**self).cmp(&**other) }
}

// FIXME (#18248) Make `T` `Sized?`
impl<T: Hash> Hash for Cc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: fmt::Display> fmt::Display for Cc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: fmt::Debug> fmt::Debug for Cc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T> fmt::Pointer for Cc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Pointer::fmt(&*self._ptr, f)
    }
}

/// A weak version of `Cc<T>`.
///
/// Weak references do not count when determining if the inner value should be
/// dropped.
///
/// See the [module level documentation](./) for more.
#[unsafe_no_drop_flag]
pub struct Weak<T> {
    // FIXME #12808: strange names to try to avoid interfering with
    // field accesses of the contained type via Deref
    _ptr: NonZero<*mut CcBox<T>>,
}

impl<T> Weak<T> {

    /// Upgrades a weak reference to a strong reference.
    ///
    /// Upgrades the `Weak<T>` reference to an `Cc<T>`, if possible.
    ///
    /// Returns `None` if there were no strong references and the data was
    /// destroyed.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// let five = Cc::new(5);
    ///
    /// let weak_five = five.downgrade();
    ///
    /// let strong_five: Option<Cc<_>> = weak_five.upgrade();
    /// ```
    pub fn upgrade(&self) -> Option<Cc<T>> {
        if self.strong() == 0 {
            None
        } else {
            self.inc_strong();
            Some(Cc { _ptr: self._ptr })
        }
    }
}

impl<T> Drop for Weak<T> {
    /// Drops the `Weak<T>`.
    ///
    /// This will decrement the weak reference count.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// {
    ///     let five = Cc::new(5);
    ///     let weak_five = five.downgrade();
    ///
    ///     // stuff
    ///
    ///     drop(weak_five); // explicit drop
    /// }
    /// {
    ///     let five = Cc::new(5);
    ///     let weak_five = five.downgrade();
    ///
    ///     // stuff
    ///
    /// } // implicit drop
    /// ```
    fn drop(&mut self) {
        unsafe {
            let ptr = *self._ptr;
            if !ptr.is_null() && ptr as usize != mem::POST_DROP_USIZE {
                self.dec_weak();
                // the weak count starts at 1, and will only go to zero if all
                // the strong pointers have disappeared.
                if self.weak() == 0 {
                    deallocate(ptr as *mut u8, size_of::<CcBox<T>>(),
                               min_align_of::<CcBox<T>>())
                }
            }
        }
    }
}

impl<T> Clone for Weak<T> {

    /// Makes a clone of the `Weak<T>`.
    ///
    /// This increases the weak reference count.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(alloc)]
    /// use bacon_rajan_cc::Cc;
    ///
    /// let weak_five = Cc::new(5).downgrade();
    ///
    /// weak_five.clone();
    /// ```
    #[inline]
    fn clone(&self) -> Weak<T> {
        self.inc_weak();
        Weak { _ptr: self._ptr }
    }
}

impl<T: fmt::Debug> fmt::Debug for Weak<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(Weak)")
    }
}

#[doc(hidden)]
trait CcBoxPtr<T> {
    fn inner(&self) -> &CcBox<T>;

    #[inline]
    fn strong(&self) -> usize { self.inner().strong.get() }

    #[inline]
    fn inc_strong(&self) { self.inner().strong.set(self.strong() + 1); }

    #[inline]
    fn dec_strong(&self) { self.inner().strong.set(self.strong() - 1); }

    #[inline]
    fn weak(&self) -> usize { self.inner().weak.get() }

    #[inline]
    fn inc_weak(&self) { self.inner().weak.set(self.weak() + 1); }

    #[inline]
    fn dec_weak(&self) { self.inner().weak.set(self.weak() - 1); }
}

impl<T> CcBoxPtr<T> for Cc<T> {
    #[inline(always)]
    fn inner(&self) -> &CcBox<T> {
        unsafe {
            // Safe to assume this here, as if it weren't true, we'd be breaking
            // the contract anyway.
            // This allows the null check to be elided in the destructor if we
            // manipulated the reference count in the same function.
            assume(!self._ptr.is_null());
            &(**self._ptr)
        }
    }
}

impl<T> CcBoxPtr<T> for Weak<T> {
    #[inline(always)]
    fn inner(&self) -> &CcBox<T> {
        unsafe {
            // Safe to assume this here, as if it weren't true, we'd be breaking
            // the contract anyway.
            // This allows the null check to be elided in the destructor if we
            // manipulated the reference count in the same function.
            assume(!self._ptr.is_null());
            &(**self._ptr)
        }
    }
}

pub type Tracer = FnMut(&CcTrace);

pub trait CcTrace: fmt::Debug {
    fn trace(&self, tracer: &mut Tracer);
}

#[cfg(test)]
mod tests {
    #![plugin(bacon_rajan_cc)]

    use super::{Cc, CcTrace, Weak, weak_count, strong_count};
    use std::boxed::Box;
    use std::cell::RefCell;
    use std::option::Option;
    use std::option::Option::{Some, None};
    use std::result::Result::{Err, Ok};
    use std::mem::drop;
    use std::clone::Clone;

    // trace_macros!(true);

    // #[derive(CcTrace, Debug)]
    // struct CycleCollected {
    //     a: Cc<u32>,
    //     b: Cc<String>,
    // }

    // trace_macros!(false);

    // #[test]
    // fn test_plugin() {
    //     let x = CycleCollected {
    //         a: Cc::new(5),
    //         b: Cc::new("hello".into()),
    //     };

    //     CcTrace::trace(&x, &mut |v| {
    //         println!("traced {:?}", v);
    //     });

    //     assert!(false);
    // }

    // Tests copied from `Rc<T>`.

    #[test]
    fn test_clone() {
        let x = Cc::new(RefCell::new(5));
        let y = x.clone();
        *x.borrow_mut() = 20;
        assert_eq!(*y.borrow(), 20);
    }

    #[test]
    fn test_simple() {
        let x = Cc::new(5);
        assert_eq!(*x, 5);
    }

    #[test]
    fn test_simple_clone() {
        let x = Cc::new(5);
        let y = x.clone();
        assert_eq!(*x, 5);
        assert_eq!(*y, 5);
    }

    #[test]
    fn test_destructor() {
        let x: Cc<Box<_>> = Cc::new(Box::new(5));
        assert_eq!(**x, 5);
    }

    #[test]
    fn test_live() {
        let x = Cc::new(5);
        let y = x.downgrade();
        assert!(y.upgrade().is_some());
    }

    #[test]
    fn test_dead() {
        let x = Cc::new(5);
        let y = x.downgrade();
        drop(x);
        assert!(y.upgrade().is_none());
    }

    #[test]
    fn weak_self_cyclic() {
        struct Cycle {
            x: RefCell<Option<Weak<Cycle>>>
        }

        let a = Cc::new(Cycle { x: RefCell::new(None) });
        let b = a.clone().downgrade();
        *a.x.borrow_mut() = Some(b);

        // hopefully we don't double-free (or leak)...
    }

    #[test]
    fn is_unique() {
        let x = Cc::new(3);
        assert!(super::is_unique(&x));
        let y = x.clone();
        assert!(!super::is_unique(&x));
        drop(y);
        assert!(super::is_unique(&x));
        let w = x.downgrade();
        assert!(!super::is_unique(&x));
        drop(w);
        assert!(super::is_unique(&x));
    }

    #[test]
    fn test_strong_count() {
        let a = Cc::new(0u32);
        assert!(strong_count(&a) == 1);
        let w = a.downgrade();
        assert!(strong_count(&a) == 1);
        let b = w.upgrade().expect("upgrade of live rc failed");
        assert!(strong_count(&b) == 2);
        assert!(strong_count(&a) == 2);
        drop(w);
        drop(a);
        assert!(strong_count(&b) == 1);
        let c = b.clone();
        assert!(strong_count(&b) == 2);
        assert!(strong_count(&c) == 2);
    }

    #[test]
    fn test_weak_count() {
        let a = Cc::new(0u32);
        assert!(strong_count(&a) == 1);
        assert!(weak_count(&a) == 0);
        let w = a.downgrade();
        assert!(strong_count(&a) == 1);
        assert!(weak_count(&a) == 1);
        drop(w);
        assert!(strong_count(&a) == 1);
        assert!(weak_count(&a) == 0);
        let c = a.clone();
        assert!(strong_count(&a) == 2);
        assert!(weak_count(&a) == 0);
        drop(c);
    }

    #[test]
    fn try_unwrap() {
        let x = Cc::new(3);
        assert_eq!(super::try_unwrap(x), Ok(3));
        let x = Cc::new(4);
        let _y = x.clone();
        assert_eq!(super::try_unwrap(x), Err(Cc::new(4)));
        let x = Cc::new(5);
        let _w = x.downgrade();
        assert_eq!(super::try_unwrap(x), Err(Cc::new(5)));
    }

    #[test]
    fn get_mut() {
        let mut x = Cc::new(3);
        *super::get_mut(&mut x).unwrap() = 4;
        assert_eq!(*x, 4);
        let y = x.clone();
        assert!(super::get_mut(&mut x).is_none());
        drop(y);
        assert!(super::get_mut(&mut x).is_some());
        let _w = x.downgrade();
        assert!(super::get_mut(&mut x).is_none());
    }

    #[test]
    fn test_cowrc_clone_make_unique() {
        let mut cow0 = Cc::new(75);
        let mut cow1 = cow0.clone();
        let mut cow2 = cow1.clone();

        assert!(75 == *cow0.make_unique());
        assert!(75 == *cow1.make_unique());
        assert!(75 == *cow2.make_unique());

        *cow0.make_unique() += 1;
        *cow1.make_unique() += 2;
        *cow2.make_unique() += 3;

        assert!(76 == *cow0);
        assert!(77 == *cow1);
        assert!(78 == *cow2);

        // none should point to the same backing memory
        assert!(*cow0 != *cow1);
        assert!(*cow0 != *cow2);
        assert!(*cow1 != *cow2);
    }

    #[test]
    fn test_cowrc_clone_unique2() {
        let mut cow0 = Cc::new(75);
        let cow1 = cow0.clone();
        let cow2 = cow1.clone();

        assert!(75 == *cow0);
        assert!(75 == *cow1);
        assert!(75 == *cow2);

        *cow0.make_unique() += 1;

        assert!(76 == *cow0);
        assert!(75 == *cow1);
        assert!(75 == *cow2);

        // cow1 and cow2 should share the same contents
        // cow0 should have a unique reference
        assert!(*cow0 != *cow1);
        assert!(*cow0 != *cow2);
        assert!(*cow1 == *cow2);
    }

    #[test]
    fn test_cowrc_clone_weak() {
        let mut cow0 = Cc::new(75);
        let cow1_weak = cow0.downgrade();

        assert!(75 == *cow0);
        assert!(75 == *cow1_weak.upgrade().unwrap());

        *cow0.make_unique() += 1;

        assert!(76 == *cow0);
        assert!(cow1_weak.upgrade().is_none());
    }

    #[test]
    fn test_show() {
        let foo = Cc::new(75);
        assert_eq!(format!("{:?}", foo), "75");
    }
}
