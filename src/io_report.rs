use std::{
  marker::{PhantomData, PhantomPinned},
  mem::MaybeUninit,
};

use core_foundation::{
  base::{kCFAllocatorDefault, CFTypeRef},
  dictionary::{CFDictionaryRef, CFMutableDictionaryRef},
  string::CFStringRef,
};

use crate::{io_kit::*, sources::from_cfstr};

pub type WithError<T> = Result<T, Box<dyn std::error::Error>>;
pub type CVoidRef = *const std::ffi::c_void;

#[repr(C)]
pub struct IOReportSubscription {
  _data: [u8; 0],
  _phantom: PhantomData<(*mut u8, PhantomPinned)>,
}

pub type IOReportSubscriptionRef = *const IOReportSubscription;

#[link(name = "IOReport", kind = "dylib")]
#[rustfmt::skip]
extern "C" {
  pub fn IOReportCopyAllChannels(a: u64, b: u64) -> CFMutableDictionaryRef;
  pub fn IOReportCopyChannelsInGroup(group: CFStringRef, subgroup: CFStringRef, c: u64, d: u64, e: u64) -> CFMutableDictionaryRef;
  pub fn IOReportMergeChannels(a: CFDictionaryRef, b: CFDictionaryRef, nil: CFTypeRef);
  pub fn IOReportCreateSubscription(a: CVoidRef, desired_channels: CFMutableDictionaryRef, subbed_channels: *mut CFMutableDictionaryRef, channel_id: u64, b: CFTypeRef) -> IOReportSubscriptionRef;
  pub fn IOReportCreateSamples(a: IOReportSubscriptionRef, b: CFMutableDictionaryRef, c: CFTypeRef) -> CFDictionaryRef;
  pub fn IOReportCreateSamplesDelta(a: CFDictionaryRef, b: CFDictionaryRef, c: CFTypeRef) -> CFDictionaryRef;
  pub fn IOReportChannelGetGroup(a: CFDictionaryRef) -> CFStringRef;
  pub fn IOReportChannelGetSubGroup(a: CFDictionaryRef) -> CFStringRef;
  pub fn IOReportChannelGetChannelName(a: CFDictionaryRef) -> CFStringRef;
  pub fn IOReportSimpleGetIntegerValue(a: CFDictionaryRef, b: i32) -> i64;
  pub fn IOReportChannelGetUnitLabel(a: CFDictionaryRef) -> CFStringRef;
  pub fn IOReportStateGetCount(a: CFDictionaryRef) -> i32;
  pub fn IOReportStateGetNameForIndex(a: CFDictionaryRef, b: i32) -> CFStringRef;
  pub fn IOReportStateGetResidency(a: CFDictionaryRef, b: i32) -> i64;
}

fn get_cf_string<F>(getter: F) -> String
where
  F: FnOnce() -> CFStringRef,
{
  match getter() {
    x if x.is_null() => String::new(),
    x => from_cfstr(x),
  }
}

pub fn cfio_get_group(item: CFDictionaryRef) -> String {
  get_cf_string(|| unsafe { IOReportChannelGetGroup(item) })
}

pub fn cfio_get_subgroup(item: CFDictionaryRef) -> String {
  get_cf_string(|| unsafe { IOReportChannelGetSubGroup(item) })
}

pub fn cfio_get_channel(item: CFDictionaryRef) -> String {
  get_cf_string(|| unsafe { IOReportChannelGetChannelName(item) })
}

pub fn cfio_get_props(entry: u32, name: String) -> WithError<CFDictionaryRef> {
  unsafe {
    let mut props: MaybeUninit<CFMutableDictionaryRef> = MaybeUninit::uninit();
    if IORegistryEntryCreateCFProperties(entry, props.as_mut_ptr(), kCFAllocatorDefault, 0) != 0 {
      return Err(format!("Failed to get properties for {}", name).into());
    }

    Ok(props.assume_init())
  }
}

pub fn cfio_get_residencies(item: CFDictionaryRef) -> Vec<(String, i64)> {
  let count = unsafe { IOReportStateGetCount(item) };
  let mut res = vec![];

  for i in 0..count {
    let name = unsafe { IOReportStateGetNameForIndex(item, i) };
    let val = unsafe { IOReportStateGetResidency(item, i) };
    res.push((from_cfstr(name), val));
  }

  res
}

pub fn cfio_watts(item: CFDictionaryRef, unit: &String, duration: u64) -> WithError<f32> {
  let val = unsafe { IOReportSimpleGetIntegerValue(item, 0) } as f32;
  let val = val / (duration as f32 / 1000.0);
  match unit.as_str() {
    "mJ" => Ok(val / 1e3f32),
    "uJ" => Ok(val / 1e6f32),
    "nJ" => Ok(val / 1e9f32),
    _ => Err(format!("Invalid energy unit: {}", unit).into()),
  }
}
