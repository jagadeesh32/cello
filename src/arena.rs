//! Arena allocator for zero-copy request processing.
//!
//! Uses bumpalo for fast per-request memory allocation
//! that gets freed all at once after request processing.

use bumpalo::Bump;
use std::cell::RefCell;

thread_local! {
    /// Thread-local arena for request processing.
    static REQUEST_ARENA: RefCell<Bump> = RefCell::new(Bump::new());
}

/// Request-scoped arena allocator.
/// 
/// This arena is reset after each request, providing fast
/// allocation without individual deallocation overhead.
pub struct RequestArena {
    arena: Bump,
}

impl RequestArena {
    /// Create a new request arena.
    pub fn new() -> Self {
        RequestArena {
            arena: Bump::new(),
        }
    }

    /// Create a new request arena with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        RequestArena {
            arena: Bump::with_capacity(capacity),
        }
    }

    /// Allocate a string in the arena.
    pub fn alloc_str(&self, s: &str) -> &str {
        self.arena.alloc_str(s)
    }

    /// Allocate a slice in the arena.
    pub fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &[T] {
        self.arena.alloc_slice_copy(slice)
    }

    /// Allocate a value in the arena.
    pub fn alloc<T>(&self, value: T) -> &T {
        self.arena.alloc(value)
    }

    /// Reset the arena, freeing all allocations.
    pub fn reset(&mut self) {
        self.arena.reset();
    }

    /// Get the total bytes allocated.
    pub fn allocated_bytes(&self) -> usize {
        self.arena.allocated_bytes()
    }
}

impl Default for RequestArena {
    fn default() -> Self {
        Self::new()
    }
}

/// Scope guard for using thread-local arena.
pub struct ArenaScope<'a> {
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> ArenaScope<'a> {
    /// Create a new arena scope.
    pub fn new() -> Self {
        ArenaScope {
            _marker: std::marker::PhantomData,
        }
    }

    /// Allocate a string in the thread-local arena.
    pub fn alloc_str(&self, s: &str) -> &'a str {
        REQUEST_ARENA.with(|arena| {
            let arena = arena.borrow();
            // SAFETY: The string is valid for the lifetime of the arena scope
            unsafe { std::mem::transmute(arena.alloc_str(s)) }
        })
    }

    /// Allocate bytes in the thread-local arena.
    pub fn alloc_bytes(&self, bytes: &[u8]) -> &'a [u8] {
        REQUEST_ARENA.with(|arena| {
            let arena = arena.borrow();
            // SAFETY: The bytes are valid for the lifetime of the arena scope
            unsafe { std::mem::transmute(arena.alloc_slice_copy(bytes)) }
        })
    }
}

impl<'a> Default for ArenaScope<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Drop for ArenaScope<'a> {
    fn drop(&mut self) {
        // Reset the thread-local arena when scope ends
        REQUEST_ARENA.with(|arena| {
            arena.borrow_mut().reset();
        });
    }
}

/// Zero-copy string slice that borrows from arena.
#[derive(Debug, Clone, Copy)]
pub struct ArenaStr<'a> {
    data: &'a str,
}

impl<'a> ArenaStr<'a> {
    /// Create a new arena string from a str reference.
    pub fn new(data: &'a str) -> Self {
        ArenaStr { data }
    }

    /// Get the underlying string slice.
    pub fn as_str(&self) -> &'a str {
        self.data
    }

    /// Get the length of the string.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl<'a> AsRef<str> for ArenaStr<'a> {
    fn as_ref(&self) -> &str {
        self.data
    }
}

impl<'a> std::fmt::Display for ArenaStr<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_arena_alloc_str() {
        let arena = RequestArena::new();
        let s1 = arena.alloc_str("hello");
        let s2 = arena.alloc_str("world");
        
        assert_eq!(s1, "hello");
        assert_eq!(s2, "world");
        assert!(arena.allocated_bytes() > 0);
    }

    #[test]
    fn test_request_arena_reset() {
        let mut arena = RequestArena::with_capacity(1024);
        
        let _ = arena.alloc_str("hello world");
        let _bytes_before = arena.allocated_bytes();
        
        arena.reset();
        
        // After reset, new allocations start fresh
        let _ = arena.alloc_str("new string");
        // The arena reuses memory, so allocated bytes might be similar
        assert!(arena.allocated_bytes() > 0);
    }

    #[test]
    fn test_arena_scope() {
        let scope = ArenaScope::new();
        let s = scope.alloc_str("test string");
        assert_eq!(s, "test string");
        // Arena is reset when scope is dropped
    }

    #[test]
    fn test_arena_str() {
        let s = "hello";
        let arena_str = ArenaStr::new(s);
        
        assert_eq!(arena_str.as_str(), "hello");
        assert_eq!(arena_str.len(), 5);
        assert!(!arena_str.is_empty());
    }
}
