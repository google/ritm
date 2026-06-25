// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use aarch64_paging::MapError;
use aarch64_paging::descriptor::Stage2Attributes;
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::{MemoryRegion, PAGE_SIZE, Stage2};
use arrayvec::ArrayVec;
use core::cmp::Ordering;
use spin::mutex::SpinMutex;

pub use memory_access::MemoryAccessWidth;

/// The maximum number of stage-2 memory access handler regions.
pub const MAX_MEMORY_ACCESS_HANDLERS: usize = 16;
const MAX_ALLOWED_RANGES: usize = 32;
const STAGE2_PAGE_SIZE: u64 = PAGE_SIZE as u64;

/// Error returned when building the stage-2 access policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stage2ConfigError {
    /// The range base IPA is not aligned to the stage-2 page size.
    RangeBaseNotPageAligned,
    /// The range size is zero.
    EmptyRange,
    /// The range size is not a multiple of the stage-2 page size.
    RangeSizeNotPageMultiple,
    /// The range end IPA overflows.
    RangeEndOverflow,
    /// The configured range cannot be represented by the page table API.
    RangeAddressNotRepresentable,
    /// The stage-2 allowed range table is full.
    TooManyAllowedRanges,
    /// The memory access handler registry is full.
    TooManyMemoryAccessHandlerRegions,
    /// A stage-2 allowed range overlaps another allowed range.
    AllowedRangeOverlap,
    /// A memory access handler range overlaps another handler range.
    MemoryAccessHandlerRangeOverlap,
    /// A stage-2 allowed range overlaps a memory access handler range.
    AllowedRangeOverlapsMemoryAccessHandler,
    /// The page table mapping operation failed.
    MapRange(MapError),
}

/// Function called to emulate a guest memory read from a handled stage-2 range.
pub type MemoryReadHandler = fn(MemoryReadAccess) -> MemoryReadResult;
/// Function called to emulate a guest memory write to a handled stage-2 range.
pub type MemoryWriteHandler = fn(MemoryWriteAccess) -> MemoryWriteResult;

/// Optional callbacks for a stage-2 memory access handler region.
#[derive(Clone, Copy)]
pub struct MemoryAccessHandler {
    /// Callback used to emulate reads from the region.
    pub read: Option<MemoryReadHandler>,
    /// Callback used to emulate writes to the region.
    pub write: Option<MemoryWriteHandler>,
}

impl MemoryAccessHandler {
    /// Creates a handler with both read and write callbacks.
    pub const fn read_write(read: MemoryReadHandler, write: MemoryWriteHandler) -> Self {
        Self {
            read: Some(read),
            write: Some(write),
        }
    }
}

/// Description of a guest memory read access handled by RITM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryReadAccess {
    /// The faulting intermediate physical address.
    pub ipa: u64,
    /// The byte offset from the start of the handled region.
    pub offset: u64,
    /// The access width decoded from the trapped instruction syndrome.
    pub width: MemoryAccessWidth,
}

/// Description of a guest memory write access handled by RITM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryWriteAccess {
    /// The faulting intermediate physical address.
    pub ipa: u64,
    /// The byte offset from the start of the handled region.
    pub offset: u64,
    /// The access width decoded from the trapped instruction syndrome.
    pub width: MemoryAccessWidth,
    /// The value written by the guest, masked to the access width.
    pub value: u64,
}

/// Result returned by a read handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryReadResult {
    /// The read was handled and should return this value to the guest.
    Value(u64),
    /// The read should be injected back to the guest as a data abort.
    Fault,
}

/// Result returned by a write handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryWriteResult {
    /// The write was handled successfully.
    Handled,
    /// The write should be injected back to the guest as a data abort.
    Fault,
}

/// Stage-2 byte range handled by memory access callbacks.
#[derive(Clone, Copy)]
pub struct MemoryAccessRegion {
    /// The first IPA covered by this region.
    pub base_ipa: u64,
    /// The size of the region in bytes.
    pub size: usize,
    /// The callbacks used to emulate accesses to the region.
    pub handler: MemoryAccessHandler,
    end_ipa: u64,
}

impl MemoryAccessRegion {
    fn new(range: Stage2Range, handler: MemoryAccessHandler) -> Self {
        Self {
            base_ipa: range.base_ipa,
            size: range.size,
            handler,
            end_ipa: range.end_ipa,
        }
    }

    /// Returns the first IPA after the end of the region.
    pub fn end_ipa(self) -> u64 {
        checked_range_end(self.base_ipa, self.size).unwrap_or(self.end_ipa)
    }

    /// Returns the byte offset of `ipa` from the start of the region.
    pub fn offset_of(self, ipa: u64) -> u64 {
        ipa - self.base_ipa
    }

    fn overlaps_range(self, range: Stage2Range) -> bool {
        page_ranges_overlap(self.base_ipa, self.end_ipa, range.base_ipa, range.end_ipa)
    }
}

/// Match returned for a handled stage-2 memory access.
#[derive(Clone, Copy)]
pub struct MemoryAccessHandlerMatch {
    /// The matched memory access handler region.
    pub region: MemoryAccessRegion,
    /// The byte offset of the access from the start of the region.
    pub offset: u64,
}

/// Registry of stage-2 memory access handler regions.
pub struct MemoryAccessHandlerRegistry {
    regions: ArrayVec<MemoryAccessRegion, MAX_MEMORY_ACCESS_HANDLERS>,
}

impl MemoryAccessHandlerRegistry {
    fn new() -> Self {
        Self {
            regions: ArrayVec::new(),
        }
    }

    fn register(&mut self, region: MemoryAccessRegion) -> Result<(), Stage2ConfigError> {
        if self.regions.is_full() {
            return Err(Stage2ConfigError::TooManyMemoryAccessHandlerRegions);
        }

        for existing in &self.regions {
            if page_ranges_overlap(
                region.base_ipa,
                region.end_ipa,
                existing.base_ipa,
                existing.end_ipa,
            ) {
                return Err(Stage2ConfigError::MemoryAccessHandlerRangeOverlap);
            }
        }

        let insert_at = self
            .regions
            .partition_point(|existing| existing.base_ipa < region.base_ipa);
        self.regions.insert(insert_at, region);
        Ok(())
    }

    /// Finds the registered memory access handler for `ipa`.
    pub fn find(&self, ipa: u64) -> Option<MemoryAccessHandlerMatch> {
        let index = self
            .regions
            .binary_search_by(|region| {
                if ipa < region.base_ipa {
                    Ordering::Greater
                } else if ipa >= region.end_ipa() {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        let region = self.regions[index];
        Some(MemoryAccessHandlerMatch {
            region,
            offset: region.offset_of(ipa),
        })
    }
}

#[derive(Clone, Copy)]
struct Stage2Range {
    base_ipa: u64,
    size: usize,
    end_ipa: u64,
}

impl Stage2Range {
    fn new(base_ipa: u64, size: usize) -> Result<Self, Stage2ConfigError> {
        validate_range(base_ipa, size)?;
        Ok(Self {
            base_ipa,
            size,
            end_ipa: checked_range_end(base_ipa, size)?,
        })
    }

    fn overlaps(self, other: Self) -> bool {
        page_ranges_overlap(self.base_ipa, self.end_ipa, other.base_ipa, other.end_ipa)
    }
}

/// Stage-2 configuration used by the hypervisor at runtime.
pub struct Stage2Config {
    /// Stage-2 identity map for guest memory that is directly accessible.
    pub idmap: SpinMutex<IdMap<Stage2>>,
    /// Memory access handlers for ranges intentionally left unmapped.
    pub memory_access_handlers: MemoryAccessHandlerRegistry,
}

/// Builder for the guest stage-2 configuration.
pub struct Stage2Builder {
    idmap: IdMap<Stage2>,
    memory_access_handlers: MemoryAccessHandlerRegistry,
    allowed_ranges: ArrayVec<Stage2Range, MAX_ALLOWED_RANGES>,
}

impl Stage2Builder {
    /// Creates an empty stage-2 builder.
    pub fn new() -> Self {
        Self {
            idmap: IdMap::new(0, Stage2),
            memory_access_handlers: MemoryAccessHandlerRegistry::new(),
            allowed_ranges: ArrayVec::new(),
        }
    }

    /// Allows the guest to access a byte range directly with `attributes`.
    ///
    /// The range must be page-aligned and its size must be a non-zero page multiple.
    pub fn allow_range(
        &mut self,
        base_ipa: u64,
        size: usize,
        attributes: Stage2Attributes,
    ) -> Result<(), Stage2ConfigError> {
        let range = Stage2Range::new(base_ipa, size)?;
        self.check_allowed_range_capacity()?;
        self.check_no_allowed_overlap(range)?;
        self.check_no_registered_overlap(range)?;

        self.idmap
            .map_range(
                &MemoryRegion::new(
                    usize::try_from(base_ipa)
                        .map_err(|_| Stage2ConfigError::RangeAddressNotRepresentable)?,
                    usize::try_from(range.end_ipa)
                        .map_err(|_| Stage2ConfigError::RangeAddressNotRepresentable)?,
                ),
                attributes,
            )
            .map_err(Stage2ConfigError::MapRange)?;
        self.track_allowed_range(range);
        Ok(())
    }

    /// Handles guest accesses to a byte range with `handler`.
    ///
    /// The range must be page-aligned and its size must be a non-zero page multiple.
    pub fn handle_range(
        &mut self,
        base_ipa: u64,
        size: usize,
        handler: MemoryAccessHandler,
    ) -> Result<(), Stage2ConfigError> {
        let range = Stage2Range::new(base_ipa, size)?;
        self.check_no_allowed_overlap(range)?;
        self.memory_access_handlers
            .register(MemoryAccessRegion::new(range, handler))
    }

    /// Builds the runtime stage-2 configuration.
    pub fn build(self) -> Stage2Config {
        Stage2Config {
            idmap: SpinMutex::new(self.idmap),
            memory_access_handlers: self.memory_access_handlers,
        }
    }

    fn check_allowed_range_capacity(&self) -> Result<(), Stage2ConfigError> {
        if self.allowed_ranges.is_full() {
            Err(Stage2ConfigError::TooManyAllowedRanges)
        } else {
            Ok(())
        }
    }

    fn track_allowed_range(&mut self, range: Stage2Range) {
        self.allowed_ranges.push(range);
    }

    fn check_no_registered_overlap(&self, range: Stage2Range) -> Result<(), Stage2ConfigError> {
        for region in &self.memory_access_handlers.regions {
            if region.overlaps_range(range) {
                return Err(Stage2ConfigError::AllowedRangeOverlapsMemoryAccessHandler);
            }
        }
        Ok(())
    }

    fn check_no_allowed_overlap(&self, range: Stage2Range) -> Result<(), Stage2ConfigError> {
        for existing in &self.allowed_ranges {
            if range.overlaps(*existing) {
                return Err(Stage2ConfigError::AllowedRangeOverlap);
            }
        }
        Ok(())
    }
}

/// Rounds `address` down to the nearest stage-2 page boundary.
pub fn align_down_to_page(address: usize) -> usize {
    address / PAGE_SIZE * PAGE_SIZE
}

/// Rounds `address` up to the nearest stage-2 page boundary.
pub fn align_up_to_page(address: usize) -> usize {
    address.next_multiple_of(PAGE_SIZE)
}

/// Converts a `usize` address to an IPA.
pub fn to_ipa(address: usize) -> u64 {
    u64::try_from(address).expect("IPA should fit in u64")
}

fn validate_range(base_ipa: u64, size: usize) -> Result<(), Stage2ConfigError> {
    if !base_ipa.is_multiple_of(STAGE2_PAGE_SIZE) {
        return Err(Stage2ConfigError::RangeBaseNotPageAligned);
    }
    if size == 0 {
        return Err(Stage2ConfigError::EmptyRange);
    }
    if !size.is_multiple_of(PAGE_SIZE) {
        return Err(Stage2ConfigError::RangeSizeNotPageMultiple);
    }
    checked_range_end(base_ipa, size)?;
    Ok(())
}

fn checked_range_end(base_ipa: u64, size: usize) -> Result<u64, Stage2ConfigError> {
    let size = u64::try_from(size).map_err(|_| Stage2ConfigError::RangeEndOverflow)?;
    base_ipa
        .checked_add(size)
        .ok_or(Stage2ConfigError::RangeEndOverflow)
}

fn page_ranges_overlap(
    first_base_ipa: u64,
    first_end_ipa: u64,
    second_base_ipa: u64,
    second_end_ipa: u64,
) -> bool {
    let (earlier_end, later_base) = if first_base_ipa <= second_base_ipa {
        (first_end_ipa, second_base_ipa)
    } else {
        (second_end_ipa, first_base_ipa)
    };

    later_base < earlier_end
}
