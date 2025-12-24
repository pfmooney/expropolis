// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Marker struct to enforce (via panic) that it is not [drop()]ed.
///
/// Certain types follow the battern of requiring some `self`-consuming method
/// be called to reach a terminal state of the item, rather than simply allowing
/// it to be dropped.  The [NoDropMarker] assists in enforcing that by panicking
/// when dropped.  This panic can be avoided by calling
/// [NoDropMarker::consume()] when the required conditions in the containing
/// struct (such as a call to its `self`-consuming method) have beem met.
///
/// # Examples
///
/// ```no_run
/// # use propolis::util::nodrop::NoDropMarker;
/// struct ConsumeMe {
///     val: u32,
///     nodrop: NoDropMarker,
/// }
/// impl ConsumeMe {
///     fn new(val: u32) -> Self {
///         Self { val, nodrop: Default::default() }
///     }
///     fn consume(self) {
///         println!("properly consumed struct with val {}", self.val);
///         let ConsumeMe { nodrop, ..} = self;
///         nodrop.consume();
///     }
/// }
///
/// fn should_pass() {
///     let data = ConsumeMe::new(4);
///     data.consume();
/// }
///
/// fn should_panic() {
///     let data = ConsumeMe::new(4);
///     drop(data);
/// }
/// ```
#[derive(Default)]
pub struct NoDropMarker;
impl NoDropMarker {
    /// Consume this marker, explicitly avoiding its panic-on-drop() behavior
    pub fn consume(self) {
        std::mem::forget(self);
    }
}
impl Drop for NoDropMarker {
    fn drop(&mut self) {
        panic!("NoDropMarker dropped without being consume()-ed");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Default)]
    struct Test {
        #[allow(unused)]
        some_data: u32,
        marker: NoDropMarker,
    }
    impl Test {
        fn consume(self) {
            let Self { marker, .. } = self;
            marker.consume();
        }
    }

    #[test]
    fn no_panic_on_consume() {
        let data = Test::default();
        data.consume();
    }

    #[test]
    #[should_panic]
    fn panic_on_drop() {
        let data = Test::default();
        drop(data);
    }
}
