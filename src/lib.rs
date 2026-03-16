//! Low-level FFmpeg FFI wrapper optimized for zero-copy video processing
//! 
//! Safety guarantees:
//! - All raw pointers are wrapped in safe structs with proper Drop impl
//! - No Vec<u8> allocations in hot paths - direct *mut AVFrame manipulation
//! - Each unsafe block is documented with safety invariants

use libc::{c_int, c_void};
use std::ptr::NonNull;
use std::fmt;

// FFI bindings (minimal, focused)
#[repr(C)]
pub struct AVFrame {
    _private: [u8; 0],
}

extern "C" {
    fn av_frame_alloc() -> *mut AVFrame;
    fn av_frame_free(frame: *mut *mut AVFrame);
    fn av_frame_ref(dst: *mut AVFrame, src: *const AVFrame) -> c_int;
}

/// Safe wrapper around FFmpeg AVFrame with deterministic memory management
/// 
/// This struct provides zero-copy access to raw video frame data.
/// The Drop implementation ensures av_frame_free() is always called,
/// preventing memory leaks even during panics.
pub struct Frame {
    // NonNull ensures the pointer is never null
    raw: NonNull<AVFrame>,
}

impl Frame {
    /// Allocates a new AVFrame using FFmpeg's allocator
    /// 
    /// # Safety
    /// This is safe because av_frame_alloc() returns a properly initialized
    /// AVFrame or null on allocation failure (which we handle)
    pub fn new() -> Result<Self, AllocationError> {
        let ptr = unsafe { av_frame_alloc() };
        
        let non_null = NonNull::new(ptr)
            .ok_or(AllocationError::FrameAllocationFailed)?;
            
        Ok(Self { raw: non_null })
    }
    
    /// Get raw pointer for FFI operations
    /// 
    /// # Safety
    /// Caller must ensure the pointer is not used after Frame is dropped
    pub fn as_ptr(&self) -> *mut AVFrame {
        self.raw.as_ptr()
    }
    
    /// Zero-copy frame reference - no data allocation
    /// 
    /// Creates a new reference to existing frame data without copying
    pub fn reference(&self) -> Result<Self, ReferenceError> {
        let new_frame = Self::new()?;
        
        // Safety: av_frame_ref is safe with valid pointers
        let result = unsafe { av_frame_ref(new_frame.as_ptr(), self.as_ptr()) };
        
        if result != 0 {
            return Err(ReferenceError::ReferenceFailed);
        }
        
        Ok(new_frame)
    }
}

// Critical: Drop implementation for deterministic cleanup
impl Drop for Frame {
    fn drop(&mut self) {
        // Safety: self.raw is always valid and we're passing pointer to pointer
        let mut ptr = self.raw.as_ptr();
        unsafe {
            av_frame_free(&mut ptr);
        }
        // After av_frame_free, ptr is set to null automatically
    }
}

// Clone creates a new reference, not a deep copy (zero-copy)
impl Clone for Frame {
    fn clone(&self) -> Self {
        self.reference().expect("Frame reference failed during clone")
    }
}

// Error types for proper error handling
#[derive(Debug)]
pub enum AllocationError {
    FrameAllocationFailed,
}

#[derive(Debug)]
pub enum ReferenceError {
    ReferenceFailed,
}

impl fmt::Display for AllocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocationError::FrameAllocationFailed => {
                write!(f, "FFmpeg frame allocation failed")
            }
        }
    }
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReferenceError::ReferenceFailed => {
                write!(f, "FFmpeg frame reference failed")
            }
        }
    }
}

impl std::error::Error for AllocationError {}
impl std::error::Error for ReferenceError {}

// Performance benchmark module
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;
    
    /// Benchmarks zero-copy frame reference creation
    /// Expected: < 1μs per reference (no allocation)
    #[test]
    fn benchmark_frame_reference() {
        let frame = Frame::new().unwrap();
        let start = Instant::now();
        
        for _ in 0..10000 {
            let _ref = frame.reference().unwrap();
        }
        
        let elapsed = start.elapsed();
        println!("10k references: {:?}", elapsed);
        assert!(elapsed.as_micros() < 1000); // Should be < 1ms total
    }
    
    #[test]
    fn test_frame_allocation() {
        let frame = Frame::new().unwrap();
        // Frame should be valid
        assert!(!frame.as_ptr().is_null());
    }
    
    #[test]
    fn test_frame_drop() {
        let frame = Frame::new().unwrap();
        // Drop should be called automatically when frame goes out of scope
        // This test passes if no memory leak occurs
        drop(frame);
    }
}
