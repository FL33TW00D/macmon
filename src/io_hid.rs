#![allow(non_upper_case_globals)]

use core_foundation::{
  array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
  base::{kCFAllocatorDefault, CFAllocatorRef, CFRelease},
  dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    CFDictionaryRef,
  },
  string::CFStringRef,
};

use crate::{
  io_report::WithError,
  sources::{cfnum, cfstr, from_cfstr},
};

#[repr(C)]
struct IOHIDServiceClient(libc::c_void);

#[repr(C)]
struct IOHIDEventSystemClient(libc::c_void);

#[repr(C)]
struct IOHIDEvent(libc::c_void);

type IOHIDServiceClientRef = *const IOHIDServiceClient;
type IOHIDEventSystemClientRef = *const IOHIDEventSystemClient;
type IOHIDEventRef = *const IOHIDEvent;

const kIOHIDEventTypeTemperature: i64 = 15;
const kIOHIDEventTypePower: i64 = 25;

#[link(name = "IOKit", kind = "framework")]
#[rustfmt::skip]
extern "C" {
  fn IOHIDEventSystemClientCreate(allocator: CFAllocatorRef) -> IOHIDEventSystemClientRef;
  fn IOHIDEventSystemClientSetMatching(client: IOHIDEventSystemClientRef, b: CFDictionaryRef) -> i32;
  fn IOHIDEventSystemClientCopyServices(a: IOHIDEventSystemClientRef) -> CFArrayRef;
  fn IOHIDServiceClientCopyProperty(a: IOHIDServiceClientRef, b: CFStringRef) -> CFStringRef;
  fn IOHIDServiceClientCopyEvent(a: IOHIDServiceClientRef, v0: i64, v1: i32, v2: i64) -> IOHIDEventRef;
  fn IOHIDEventGetFloatValue(event: IOHIDEventRef, field: i32) -> f64;
}

// MARK: IOHIDSensors

pub struct IOHIDSensors {
  sensors: CFDictionaryRef,
}

impl IOHIDSensors {
  const PRIMARY_USAGE_PAGE: &'static str = "PrimaryUsagePage";
  const PRIMARY_USAGE: &'static str = "PrimaryUsage";
  const kHIDPage_AppleVendor: i32 = 0xff00;
  const kHIDUsage_AppleVendor_TemperatureSensor: i32 = 0x0005;

  pub fn new() -> WithError<Self> {
    let keys = [cfstr(Self::PRIMARY_USAGE_PAGE), cfstr(Self::PRIMARY_USAGE)];
    let nums =
      [cfnum(Self::kHIDPage_AppleVendor), cfnum(Self::kHIDUsage_AppleVendor_TemperatureSensor)];

    let dict = unsafe {
      CFDictionaryCreate(
        kCFAllocatorDefault,
        keys.as_ptr() as _,
        nums.as_ptr() as _,
        2,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks,
      )
    };

    Ok(Self { sensors: dict })
  }

  pub fn get_metrics(&self) -> Vec<(String, f32)> {
    unsafe {
      let system = match IOHIDEventSystemClientCreate(kCFAllocatorDefault) {
        x if x.is_null() => return vec![],
        x => x,
      };

      IOHIDEventSystemClientSetMatching(system, self.sensors);

      let services = match IOHIDEventSystemClientCopyServices(system) {
        x if x.is_null() => return vec![],
        x => x,
      };

      let mut items = vec![] as Vec<(String, f32)>;
      for i in 0..CFArrayGetCount(services) {
        let sc = match CFArrayGetValueAtIndex(services, i) as IOHIDServiceClientRef {
          x if x.is_null() => continue,
          x => x,
        };

        let name = match IOHIDServiceClientCopyProperty(sc, cfstr("Product")) {
          x if x.is_null() => continue,
          x => from_cfstr(x),
        };

        let event = match IOHIDServiceClientCopyEvent(sc, kIOHIDEventTypeTemperature, 0, 0) {
          x if x.is_null() => continue,
          x => x,
        };

        let temp = IOHIDEventGetFloatValue(event, (kIOHIDEventTypeTemperature << 16) as i32);
        CFRelease(event as _);
        items.push((name, temp as f32));
      }

      CFRelease(services as _);
      CFRelease(system as _);

      items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
      items
    }
  }
}

impl Drop for IOHIDSensors {
  fn drop(&mut self) {
    unsafe {
      CFRelease(self.sensors as _);
    }
  }
}
