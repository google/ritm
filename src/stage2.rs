// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use aarch64_paging::descriptor::Stage2Attributes;
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::{MemoryRegion, PAGE_SIZE, Stage2};
use spin::mutex::SpinMutex;

pub use crate::memory_access::MemoryAccessWidth;

pub const MAX_MEMORY_ACCESS_HANDLERS: usize = 16;
const MAX_ALLOWED_RANGES: usize = 32;
const STAGE2_PAGE_SIZE: u64 = PAGE_SIZE as u64;

pub type MemoryReadHandler = fn(MemoryReadAccess) -> MemoryReadResult;
pub type MemoryWriteHandler = fn(MemoryWriteAccess) -> MemoryWriteResult;

#[derive(Clone, Copy)]
pub struct MemoryAccessHandler {
    pub read: Option<MemoryReadHandler>,
    pub write: Option<MemoryWriteHandler>,
}

impl MemoryAccessHandler {
    pub const fn read_write(read: MemoryReadHandler, write: MemoryWriteHandler) -> Self {
        Self {
            read: Some(read),
            write: Some(write),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryReadAccess {
    pub ipa: u64,
    pub offset: u64,
    pub width: MemoryAccessWidth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryWriteAccess {
    pub ipa: u64,
    pub offset: u64,
    pub width: MemoryAccessWidth,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryReadResult {
    Value(u64),
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryWriteResult {
    Handled,
    Fault,
}

#[derive(Clone, Copy)]
pub struct MemoryAccessPageRegion {
    pub base_ipa: u64,
    pub page_count: usize,
    pub handler: MemoryAccessHandler,
}

impl MemoryAccessPageRegion {
    pub fn end_ipa(self) -> u64 {
        checked_page_range_end(self.base_ipa, self.page_count)
    }

    pub fn offset_of(self, ipa: u64) -> u64 {
        ipa - self.base_ipa
    }
}

#[derive(Clone, Copy)]
pub struct MemoryAccessHandlerMatch {
    pub region: MemoryAccessPageRegion,
    pub offset: u64,
}

pub struct MemoryAccessHandlerRegistry {
    regions: [Option<MemoryAccessPageRegion>; MAX_MEMORY_ACCESS_HANDLERS],
    len: usize,
}

impl MemoryAccessHandlerRegistry {
    const fn new() -> Self {
        Self {
            regions: [None; MAX_MEMORY_ACCESS_HANDLERS],
            len: 0,
        }
    }

    fn register(&mut self, region: MemoryAccessPageRegion) {
        assert!(
            self.len < MAX_MEMORY_ACCESS_HANDLERS,
            "too many memory access handler regions"
        );

        let mut insert_at = self.len;
        for index in 0..self.len {
            let existing =
                self.regions[index].expect("registered memory access region should exist");
            assert!(
                !page_ranges_overlap(
                    region.base_ipa,
                    region.page_count,
                    existing.base_ipa,
                    existing.page_count,
                ),
                "memory access handler ranges overlap",
            );
            if region.base_ipa < existing.base_ipa && insert_at == self.len {
                insert_at = index;
            }
        }

        for index in (insert_at..self.len).rev() {
            self.regions[index + 1] = self.regions[index];
        }
        self.regions[insert_at] = Some(region);
        self.len += 1;
    }

    pub fn find(&self, ipa: u64) -> Option<MemoryAccessHandlerMatch> {
        let mut left = 0;
        let mut right = self.len;

        while left < right {
            let middle = left + (right - left) / 2;
            let region =
                self.regions[middle].expect("registered memory access region should exist");
            if ipa < region.base_ipa {
                right = middle;
            } else if ipa >= region.end_ipa() {
                left = middle + 1;
            } else {
                return Some(MemoryAccessHandlerMatch {
                    region,
                    offset: region.offset_of(ipa),
                });
            }
        }

        None
    }
}

#[derive(Clone, Copy)]
struct PageRange {
    base_ipa: u64,
    page_count: usize,
}

pub struct Stage2Config {
    pub idmap: SpinMutex<IdMap<Stage2>>,
    pub memory_access_handlers: MemoryAccessHandlerRegistry,
}

pub struct Stage2Builder {
    idmap: IdMap<Stage2>,
    memory_access_handlers: MemoryAccessHandlerRegistry,
    allowed_ranges: [Option<PageRange>; MAX_ALLOWED_RANGES],
    allowed_range_count: usize,
}

impl Stage2Builder {
    pub fn new() -> Self {
        Self {
            idmap: IdMap::new(0, Stage2),
            memory_access_handlers: MemoryAccessHandlerRegistry::new(),
            allowed_ranges: [None; MAX_ALLOWED_RANGES],
            allowed_range_count: 0,
        }
    }

    pub fn allow_pages(&mut self, base_ipa: u64, page_count: usize, attributes: Stage2Attributes) {
        validate_page_range(base_ipa, page_count);
        self.check_no_registered_overlap(base_ipa, page_count);
        self.track_allowed_range(base_ipa, page_count);

        let end_ipa = checked_page_range_end(base_ipa, page_count);
        self.idmap
            .map_range(
                &MemoryRegion::new(
                    usize::try_from(base_ipa).expect("base IPA should fit in usize"),
                    usize::try_from(end_ipa).expect("end IPA should fit in usize"),
                ),
                attributes,
            )
            .expect("failed to map stage-2 range");
    }

    pub fn handle_pages(&mut self, base_ipa: u64, page_count: usize, handler: MemoryAccessHandler) {
        validate_page_range(base_ipa, page_count);
        self.check_no_allowed_overlap(base_ipa, page_count);
        self.memory_access_handlers
            .register(MemoryAccessPageRegion {
                base_ipa,
                page_count,
                handler,
            });
    }

    pub fn build(self) -> Stage2Config {
        Stage2Config {
            idmap: SpinMutex::new(self.idmap),
            memory_access_handlers: self.memory_access_handlers,
        }
    }

    fn track_allowed_range(&mut self, base_ipa: u64, page_count: usize) {
        assert!(
            self.allowed_range_count < MAX_ALLOWED_RANGES,
            "too many stage-2 allowed ranges"
        );
        self.allowed_ranges[self.allowed_range_count] = Some(PageRange {
            base_ipa,
            page_count,
        });
        self.allowed_range_count += 1;
    }

    fn check_no_registered_overlap(&self, base_ipa: u64, page_count: usize) {
        self.check_no_allowed_overlap(base_ipa, page_count);
        for index in 0..self.memory_access_handlers.len {
            let region = self.memory_access_handlers.regions[index]
                .expect("registered memory access region should exist");
            assert!(
                !page_ranges_overlap(base_ipa, page_count, region.base_ipa, region.page_count),
                "stage-2 allowed range overlaps a memory access handler range",
            );
        }
    }

    fn check_no_allowed_overlap(&self, base_ipa: u64, page_count: usize) {
        for index in 0..self.allowed_range_count {
            let existing = self.allowed_ranges[index].expect("allowed range should exist");
            assert!(
                !page_ranges_overlap(base_ipa, page_count, existing.base_ipa, existing.page_count),
                "stage-2 allowed ranges overlap",
            );
        }
    }
}

fn validate_page_range(base_ipa: u64, page_count: usize) {
    assert!(
        base_ipa.is_multiple_of(STAGE2_PAGE_SIZE),
        "stage-2 range base must be page-aligned",
    );
    assert!(
        page_count > 0,
        "stage-2 range must include at least one page"
    );
    checked_page_range_end(base_ipa, page_count);
}

fn checked_page_range_end(base_ipa: u64, page_count: usize) -> u64 {
    let size = u64::try_from(page_count)
        .expect("page count should fit in u64")
        .checked_mul(STAGE2_PAGE_SIZE)
        .expect("stage-2 range size overflow");
    base_ipa
        .checked_add(size)
        .expect("stage-2 range end overflow")
}

fn page_ranges_overlap(
    first_base_ipa: u64,
    first_page_count: usize,
    second_base_ipa: u64,
    second_page_count: usize,
) -> bool {
    let first_end = checked_page_range_end(first_base_ipa, first_page_count);
    let second_end = checked_page_range_end(second_base_ipa, second_page_count);
    let (earlier_end, later_base) = if first_base_ipa <= second_base_ipa {
        (first_end, second_base_ipa)
    } else {
        (second_end, first_base_ipa)
    };

    later_base < earlier_end
}
