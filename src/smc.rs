use std::{
  collections::HashMap,
  mem::{size_of, MaybeUninit},
  os::raw::c_void,
  ptr::null,
};

use core_foundation::{
  array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
  base::{kCFAllocatorDefault, kCFAllocatorNull, CFAllocatorRef, CFRange, CFRelease, CFTypeRef},
  data::{CFDataGetBytes, CFDataGetLength, CFDataRef},
  dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    CFDictionaryCreateMutableCopy, CFDictionaryGetCount, CFDictionaryGetKeysAndValues,
    CFDictionaryGetValue, CFDictionaryRef, CFMutableDictionaryRef,
  },
  number::{kCFNumberSInt32Type, CFNumberCreate, CFNumberRef},
  string::{kCFStringEncodingUTF8, CFStringCreateWithBytesNoCopy, CFStringGetCString, CFStringRef},
};

use crate::{
  io_report::WithError,
  sources::{IOConnectCallStructMethod, IOServiceClose, IOServiceOpen},
};

#[link(name = "IOKit", kind = "framework")]
#[rustfmt::skip]
extern "C" {
  fn mach_task_self() -> u32;
  fn IOServiceMatching(name: *const i8) -> CFMutableDictionaryRef;
  fn IOServiceGetMatchingServices(mainPort: u32, matching: CFDictionaryRef, existing: *mut u32) -> i32;
  fn IOIteratorNext(iterator: u32) -> u32;
  fn IORegistryEntryGetName(entry: u32, name: *mut i8) -> i32;
  fn IORegistryEntryCreateCFProperties(entry: u32, properties: *mut CFMutableDictionaryRef, allocator: CFAllocatorRef, options: u32) -> i32;
  fn IOObjectRelease(obj: u32) -> u32;
}

pub struct IOServiceIterator {
  existing: u32,
}

impl IOServiceIterator {
  pub fn new(service_name: &str) -> WithError<Self> {
    let service_name = std::ffi::CString::new(service_name).unwrap();
    let existing = unsafe {
      let service = IOServiceMatching(service_name.as_ptr() as _);
      let mut existing = 0;
      if IOServiceGetMatchingServices(0, service, &mut existing) != 0 {
        return Err(format!("{} not found", service_name.to_string_lossy()).into());
      }
      existing
    };

    Ok(Self { existing })
  }
}

impl Drop for IOServiceIterator {
  fn drop(&mut self) {
    unsafe {
      IOObjectRelease(self.existing);
    }
  }
}

impl Iterator for IOServiceIterator {
  type Item = (u32, String);

  fn next(&mut self) -> Option<Self::Item> {
    let next = unsafe { IOIteratorNext(self.existing) };
    if next == 0 {
      return None;
    }

    let mut name = [0; 128]; // 128 defined in apple docs
    if unsafe { IORegistryEntryGetName(next, name.as_mut_ptr()) } != 0 {
      return None;
    }

    let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) };
    let name = name.to_string_lossy().to_string();
    Some((next, name))
  }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyInfo {
  pub data_size: u32,
  pub data_type: u32,
  pub data_attributes: u8,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct KeyDataVer {
  pub major: u8,
  pub minor: u8,
  pub build: u8,
  pub reserved: u8,
  pub release: u16,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct PLimitData {
  pub version: u16,
  pub length: u16,
  pub cpu_p_limit: u32,
  pub gpu_p_limit: u32,
  pub mem_p_limit: u32,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct KeyData {
  pub key: u32,
  pub vers: KeyDataVer,
  pub p_limit_data: PLimitData,
  pub key_info: KeyInfo,
  pub result: u8,
  pub status: u8,
  pub data8: u8,
  pub data32: u32,
  pub bytes: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct SensorVal {
  pub name: String,
  pub unit: String,
  pub data: Vec<u8>,
}

pub struct SMC {
  conn: u32,
  keys: HashMap<u32, KeyInfo>,
}

impl SMC {
  pub fn new() -> WithError<Self> {
    let mut conn = 0;

    for (device, name) in IOServiceIterator::new("AppleSMC")? {
      if name == "AppleSMCKeysEndpoint" {
        let rs = unsafe { IOServiceOpen(device, mach_task_self(), 0, &mut conn) };
        if rs != 0 {
          return Err(format!("IOServiceOpen: {}", rs).into());
        }
      }
    }

    Ok(Self { conn, keys: HashMap::new() })
  }

  fn read(&self, input: &KeyData) -> WithError<KeyData> {
    let ival = input as *const _ as _;
    let ilen = size_of::<KeyData>();
    let mut oval = KeyData::default();
    let mut olen = size_of::<KeyData>();

    let rs = unsafe {
      IOConnectCallStructMethod(self.conn, 2, ival, ilen, &mut oval as *mut _ as _, &mut olen)
    };

    if rs != 0 {
      // println!("{:?}", input);
      return Err(format!("IOConnectCallStructMethod: {}", rs).into());
    }

    if oval.result == 132 {
      return Err("SMC key not found".into());
    }

    if oval.result != 0 {
      return Err(format!("SMC error: {}", oval.result).into());
    }

    Ok(oval)
  }

  pub fn key_by_index(&self, index: u32) -> WithError<String> {
    let ival = KeyData { data8: 8, data32: index, ..Default::default() };
    let oval = self.read(&ival)?;
    Ok(std::str::from_utf8(&oval.key.to_be_bytes()).unwrap().to_string())
  }

  pub fn read_key_info(&mut self, key: &str) -> WithError<KeyInfo> {
    if key.len() != 4 {
      return Err("SMC key must be 4 bytes long".into());
    }

    // key is FourCC
    let key = key.bytes().fold(0, |acc, x| (acc << 8) + x as u32);
    if let Some(ki) = self.keys.get(&key) {
      // println!("cache hit for {}", key);
      return Ok(ki.clone());
    }

    let ival = KeyData { data8: 9, key, ..Default::default() };
    let oval = self.read(&ival)?;
    self.keys.insert(key, oval.key_info);
    Ok(oval.key_info)
  }

  pub fn read_val(&mut self, key: &str) -> WithError<SensorVal> {
    let name = key.to_string();

    let key_info = self.read_key_info(key)?;
    let key = key.bytes().fold(0, |acc, x| (acc << 8) + x as u32);
    // println!("{:?}", key_info);

    let ival = KeyData { data8: 5, key, key_info, ..Default::default() };
    let oval = self.read(&ival)?;
    // println!("{:?}", oval.bytes);

    Ok(SensorVal {
      name,
      unit: std::str::from_utf8(&key_info.data_type.to_be_bytes()).unwrap().to_string(),
      data: oval.bytes[0..key_info.data_size as usize].to_vec(),
    })
  }

  pub fn read_all_keys(&mut self) -> WithError<Vec<String>> {
    let val = self.read_val("#KEY")?;
    let val = u32::from_be_bytes(val.data[0..4].try_into().unwrap());

    let mut keys = Vec::new();
    for i in 0..val {
      let key = self.key_by_index(i)?;
      let val = self.read_val(&key);
      if val.is_err() {
        continue;
      }

      let val = val.unwrap();
      keys.push(val.name);
    }

    Ok(keys)
  }
}

impl Drop for SMC {
  fn drop(&mut self) {
    unsafe {
      IOServiceClose(self.conn);
    }
  }
}
