//! Byte ring for IRQ UART. Capacity is N-1 bytes.

#[derive(Clone, Copy)]
pub struct Ring<const N: usize> {
    buf: [u8; N],
    head: usize,
    tail: usize,
}

impl<const N: usize> Ring<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            head: 0,
            tail: 0,
        }
    }

    pub const fn cap(&self) -> usize {
        N.saturating_sub(1)
    }

    pub fn len(&self) -> usize {
        self.head.wrapping_sub(self.tail) % N
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn push(&mut self, b: u8) -> bool {
        let next = (self.head + 1) % N;
        if next == self.tail {
            return false;
        }
        self.buf[self.head] = b;
        self.head = next;
        true
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.is_empty() {
            return None;
        }
        let b = self.buf[self.tail];
        self.tail = (self.tail + 1) % N;
        Some(b)
    }

    pub fn pop_chunk(&mut self, out: &mut [u8]) -> usize {
        let mut n = 0;
        while n < out.len() {
            match self.pop() {
                Some(b) => {
                    out[n] = b;
                    n += 1;
                }
                None => break,
            }
        }
        n
    }

    pub fn push_slice(&mut self, data: &[u8]) -> usize {
        let mut n = 0;
        for &b in data {
            if !self.push(b) {
                break;
            }
            n += 1;
        }
        n
    }
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_order() {
        let mut r = Ring::<8>::new();
        assert!(r.push(1));
        assert!(r.push(2));
        assert_eq!(r.pop(), Some(1));
        assert_eq!(r.pop(), Some(2));
        assert_eq!(r.pop(), None);
    }

    #[test]
    fn cap_is_n_minus_one() {
        let mut r = Ring::<16>::new();
        assert_eq!(r.cap(), 15);
        for i in 0..15 {
            assert!(r.push(i as u8), "i={i}");
        }
        assert!(!r.push(99));
        assert_eq!(r.pop(), Some(0));
        assert!(r.push(99));
    }

    #[test]
    fn chunk_and_slice() {
        let mut r = Ring::<32>::new();
        assert_eq!(r.push_slice(b"hello"), 5);
        let mut out = [0u8; 8];
        assert_eq!(r.pop_chunk(&mut out), 5);
        assert_eq!(&out[..5], b"hello");
    }
}
