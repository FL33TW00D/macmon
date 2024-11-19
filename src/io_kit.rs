#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use core_foundation::{
  base::CFAllocatorRef,
  dictionary::{CFDictionaryRef, CFMutableDictionaryRef},
};

// Common IOKit functions

#[link(name = "IOKit", kind = "framework")]
#[rustfmt::skip]
extern "C" {
  pub fn IOServiceMatching(name: *const i8) -> CFMutableDictionaryRef;
  pub fn IOServiceGetMatchingServices(mainPort: u32, matching: CFDictionaryRef, existing: *mut u32) -> i32;
  pub fn IOIteratorNext(iterator: u32) -> u32;
  pub fn IORegistryEntryGetName(entry: u32, name: *mut i8) -> i32;
  pub fn IORegistryEntryCreateCFProperties(entry: u32, properties: *mut CFMutableDictionaryRef, allocator: CFAllocatorRef, options: u32) -> i32;
  pub fn IOObjectRelease(obj: u32) -> u32;
}
