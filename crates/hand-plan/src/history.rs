// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use std::rc::Rc;
struct Node<T> {
    parent: Option<Rc<Node<T>>>,
    value: T,
}
impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        // Unique tails can be thousands of entries long. Peel them iteratively
        // rather than overflowing the native/WASM stack in recursive Rc drops.
        let mut next = self.parent.take();
        while let Some(parent) = next {
            match Rc::try_unwrap(parent) {
                Ok(mut node) => next = node.parent.take(),
                Err(_) => break,
            }
        }
    }
}
pub(crate) struct History<T>(Option<Rc<Node<T>>>);
impl<T> Clone for History<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Default for History<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<T> History<T> {
    pub fn push(&mut self, value: T) {
        self.0 = Some(Rc::new(Node {
            parent: self.0.clone(),
            value,
        }));
    }
    pub fn backwards(&self) -> impl Iterator<Item = &T> {
        let mut next = self.0.as_deref();
        std::iter::from_fn(move || {
            let node = next?;
            next = node.parent.as_deref();
            Some(&node.value)
        })
    }
}
impl<T: Copy> History<T> {
    pub fn values(&self) -> Vec<T> {
        let mut v = self.backwards().copied().collect::<Vec<_>>();
        v.reverse();
        v
    }
    pub fn suffix(&self, keep: impl Fn(&T) -> bool) -> Vec<T> {
        let mut v = self
            .backwards()
            .take_while(|v| keep(v))
            .copied()
            .collect::<Vec<_>>();
        v.reverse();
        v
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branches_are_immutable_and_long_unique_tails_drop_without_recursion() {
        let mut a = History::default();
        a.push(1);
        let mut b = a.clone();
        b.push(2);
        assert_eq!(a.values(), [1]);
        assert_eq!(b.values(), [1, 2]);
        for i in 0..20000 {
            a.push(i);
        }
        drop(a);
        assert_eq!(b.values(), [1, 2]);
    }
}
