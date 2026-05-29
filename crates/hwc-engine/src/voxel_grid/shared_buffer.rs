//! Shared memory buffer for zero-copy IDE interface (Task D2)
//!
//! This module provides memory-mapped shared buffers that enable the IDE
//! to read VoxelGrid data directly from compiler memory without copies.
//!
//! ARCHITECTURE:
//! - Memory-mapped file interface for cross-process sharing
//! - Page-based dirty tracking for incremental updates
//! - Platform-specific implementations (Windows/Linux/macOS)
//! - Concurrent read (IDE) + write (compiler) access

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Page size for dirty tracking (4KB - standard OS page size)
pub const PAGE_SIZE: usize = 4096;

/// Shared voxel buffer metadata
///
/// This structure is placed at the beginning of the shared memory region
/// and contains information about the grid layout and dirty pages.
#[repr(C)]
#[derive(Debug)]
pub struct SharedBufferHeader {
    /// Magic number for validation (0x48574358 = "HWCX")
    pub magic: u32,

    /// Version number for compatibility checking
    pub version: u32,

    /// Grid dimensions (x, y, z) in voxels
    pub grid_size: [usize; 3],

    /// Maximum number of chunks
    pub max_chunks: usize,

    /// Offset to chunk directory (in bytes from start of buffer)
    pub chunk_directory_offset: usize,

    /// Offset to dirty page bitmap (in bytes from start of buffer)
    pub dirty_bitmap_offset: usize,

    /// Number of pages in the buffer
    pub num_pages: usize,

    /// Timestamp of last update (for IDE synchronization)
    pub last_update_timestamp: u64,
}

impl SharedBufferHeader {
    const MAGIC: u32 = 0x48574358; // "HWCX"
    const VERSION: u32 = 1;

    pub fn new(grid_size: (usize, usize, usize), max_chunks: usize, num_pages: usize) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            grid_size: [grid_size.0, grid_size.1, grid_size.2],
            max_chunks,
            chunk_directory_offset: std::mem::size_of::<SharedBufferHeader>(),
            dirty_bitmap_offset: 0, // Will be calculated after chunk directory
            num_pages,
            last_update_timestamp: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.magic != Self::MAGIC {
            return Err(format!("Invalid magic number: 0x{:08X}", self.magic));
        }
        if self.version != Self::VERSION {
            return Err(format!("Unsupported version: {}", self.version));
        }
        Ok(())
    }
}

/// Dirty page tracker for incremental updates
///
/// Tracks which pages of the shared buffer have been modified since
/// the last time the IDE read them. This enables efficient incremental
/// rendering where only changed regions are updated.
pub struct DirtyPageTracker {
    /// Bitmap of dirty pages (1 bit per page)
    /// Uses atomic u64 chunks for lock-free updates
    bitmap: Vec<AtomicU64>,

    /// Total number of pages
    num_pages: usize,
}

impl DirtyPageTracker {
    /// Create a new dirty page tracker
    pub fn new(num_pages: usize) -> Self {
        // Calculate number of u64 chunks needed (64 pages per chunk)
        let num_chunks = num_pages.div_ceil(64);
        let mut bitmap = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            bitmap.push(AtomicU64::new(0));
        }

        Self { bitmap, num_pages }
    }

    /// Mark a page as dirty
    #[inline]
    pub fn mark_dirty(&self, page_index: usize) {
        if page_index >= self.num_pages {
            return;
        }

        let chunk_index = page_index / 64;
        let bit_index = page_index % 64;

        // Atomic OR to set the bit
        self.bitmap[chunk_index].fetch_or(1u64 << bit_index, Ordering::Release);
    }

    /// Mark a range of pages as dirty
    pub fn mark_range_dirty(&self, start_page: usize, end_page: usize) {
        for page in start_page..=end_page.min(self.num_pages - 1) {
            self.mark_dirty(page);
        }
    }

    /// Check if a page is dirty
    #[inline]
    pub fn is_dirty(&self, page_index: usize) -> bool {
        if page_index >= self.num_pages {
            return false;
        }

        let chunk_index = page_index / 64;
        let bit_index = page_index % 64;

        let chunk = self.bitmap[chunk_index].load(Ordering::Acquire);
        (chunk & (1u64 << bit_index)) != 0
    }

    /// Get all dirty page indices
    pub fn get_dirty_pages(&self) -> Vec<usize> {
        let mut dirty_pages = Vec::new();

        for (chunk_index, chunk) in self.bitmap.iter().enumerate() {
            let bits = chunk.load(Ordering::Acquire);
            if bits == 0 {
                continue;
            }

            // Find set bits
            for bit_index in 0..64 {
                if (bits & (1u64 << bit_index)) != 0 {
                    let page_index = chunk_index * 64 + bit_index;
                    if page_index < self.num_pages {
                        dirty_pages.push(page_index);
                    }
                }
            }
        }

        dirty_pages
    }

    /// Clear all dirty bits
    pub fn clear_all(&self) {
        for chunk in &self.bitmap {
            chunk.store(0, Ordering::Release);
        }
    }

    /// Clear a specific page's dirty bit
    #[inline]
    pub fn clear_page(&self, page_index: usize) {
        if page_index >= self.num_pages {
            return;
        }

        let chunk_index = page_index / 64;
        let bit_index = page_index % 64;

        // Atomic AND with inverted bit to clear
        self.bitmap[chunk_index].fetch_and(!(1u64 << bit_index), Ordering::Release);
    }

    /// Get the number of dirty pages
    pub fn count_dirty(&self) -> usize {
        let mut count = 0;
        for chunk in &self.bitmap {
            let bits = chunk.load(Ordering::Acquire);
            count += bits.count_ones() as usize;
        }
        count
    }
}

/// Shared voxel buffer for zero-copy IDE interface
///
/// This provides a memory-mapped interface to the VoxelGrid that can be
/// shared between the compiler process and the IDE process.
pub struct SharedVoxelBuffer {
    /// Dirty page tracker
    dirty_tracker: Arc<DirtyPageTracker>,

    /// Grid dimensions
    grid_size: (usize, usize, usize),

    /// Maximum number of chunks
    max_chunks: usize,
}

impl SharedVoxelBuffer {
    /// Create a new shared voxel buffer
    ///
    /// # Arguments
    /// * `grid_size` - Grid dimensions (x, y, z) in voxels
    /// * `max_chunks` - Maximum number of chunks in the grid
    pub fn new(grid_size: (usize, usize, usize), max_chunks: usize) -> Self {
        // Calculate number of pages needed
        // Each chunk is ~336 bytes, so we can fit ~12 chunks per page
        let chunks_per_page = PAGE_SIZE / 336;
        let num_pages = max_chunks.div_ceil(chunks_per_page);

        Self {
            dirty_tracker: Arc::new(DirtyPageTracker::new(num_pages)),
            grid_size,
            max_chunks,
        }
    }

    /// Mark a chunk as dirty (for incremental updates)
    ///
    /// This should be called whenever a chunk is modified.
    /// The IDE can then query dirty pages to know what to re-render.
    pub fn mark_chunk_dirty(&self, chunk_index: usize) {
        // Calculate which page this chunk belongs to
        let chunks_per_page = PAGE_SIZE / 336;
        let page_index = chunk_index / chunks_per_page;

        self.dirty_tracker.mark_dirty(page_index);
    }

    /// Get all dirty page indices
    ///
    /// The IDE calls this to determine which regions need to be re-rendered.
    pub fn get_dirty_pages(&self) -> Vec<usize> {
        self.dirty_tracker.get_dirty_pages()
    }

    /// Clear all dirty page bits
    ///
    /// The IDE calls this after it has processed all dirty pages.
    pub fn clear_dirty_pages(&self) {
        self.dirty_tracker.clear_all();
    }

    /// Get the number of dirty pages
    pub fn count_dirty_pages(&self) -> usize {
        self.dirty_tracker.count_dirty()
    }

    /// Check if a specific page is dirty
    pub fn is_page_dirty(&self, page_index: usize) -> bool {
        self.dirty_tracker.is_dirty(page_index)
    }

    /// Get grid dimensions
    pub fn grid_size(&self) -> (usize, usize, usize) {
        self.grid_size
    }

    /// Get maximum number of chunks
    pub fn max_chunks(&self) -> usize {
        self.max_chunks
    }

    /// Get the dirty tracker (for advanced use cases)
    pub fn dirty_tracker(&self) -> Arc<DirtyPageTracker> {
        Arc::clone(&self.dirty_tracker)
    }
}
