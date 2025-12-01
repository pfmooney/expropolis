// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::{self, Debug};
use std::sync::Condvar as StdCondvar;
use std::sync::Mutex as StdMutex;
use std::sync::TryLockError;

/// Infallible wrapper for [std::sync::Mutex]
#[repr(transparent)]
pub struct Mutex<T: ?Sized>(StdMutex<T>);

// Re-export `MutexGuard` directly
pub use std::sync::MutexGuard;

impl<T> Mutex<T> {
    /// Create a new mutex in an unlocked state, ready for use.
    pub fn new(data: T) -> Self {
        Self(StdMutex::new(data))
    }

    pub fn into_inner(self) -> T {
        if let Ok(this) = self.0.into_inner() {
            this
        } else {
            panic!("poisoned mutex");
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Infallible equivalent to [std::sync::Mutex::lock()]
    ///
    /// Will panic if the underlying Mutex becomes poisoned, but frees the
    /// caller from having to check or unwrap() a [std::sync::LockResult].
    pub fn lock(&self) -> MutexGuard<'_, T> {
        if let Ok(guard) = self.0.lock() {
            guard
        } else {
            panic!("poisoned mutex");
        }
    }

    /// Infallible equivalent to [std::sync::Mutex::try_lock()]
    ///
    /// Returns `Some` if the mutex was able to be acquired, `None` if the mutex
    /// could not be acquired (held by another thread), and panics if the mutex
    /// is poisoned.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        match self.0.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(_)) => {
                panic!("poisoned mutex");
            }
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        if let Ok(this) = self.0.get_mut() {
            this
        } else {
            panic!("poisoned mutex");
        }
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: ?Sized + Debug> Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Mutex::new(value)
    }
}

/// Infallible wrapper for [std::sync::Condvar]
#[repr(transparent)]
pub struct Condvar(StdCondvar);

impl Condvar {
    /// Creates a new condition variable which is ready to be waited on and
    /// notified.
    pub fn new() -> Self {
        Self(StdCondvar::new())
    }

    /// Infallible equivalent to [std::sync::Condvar::wait_while()]
    ///
    /// Will panic if the underlying Mutex becomes poisoned, but frees the
    /// caller from having to check or unwrap() a [std::sync::LockResult].
    pub fn wait_while<'a, T, F>(
        &self,
        guard: MutexGuard<'a, T>,
        condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        if let Ok(guard) = self.0.wait_while(guard, condition) {
            guard
        } else {
            panic!("poisoned mutex");
        }
    }

    /// Infallible equivalent to [std::sync::Condvar::wait()]
    ///
    /// Will panic if the underlying Mutex becomes poisoned, but frees the
    /// caller from having to check or unwrap() a [std::sync::LockResult].
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        if let Ok(guard) = self.0.wait(guard) {
            guard
        } else {
            panic!("poisoned mutex");
        }
    }

    pub fn notify_one(&self) {
        self.0.notify_one()
    }

    pub fn notify_all(&self) {
        self.0.notify_all()
    }
}
