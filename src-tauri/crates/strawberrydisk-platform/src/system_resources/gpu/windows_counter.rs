//! Persistent PDH query and bounded decoding shared by GPU activity and adapter memory.
use super::windows::{check, engine, Engine, Failure, MAX_BUFFER_BYTES, PDH_FMT_NOCAP100};
use std::{collections::HashMap, mem, ptr, time::Instant};
use windows_sys::{core::w, Win32::System::Performance::*};

pub(super) struct Query {
    pub(super) handle: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    pub(super) previous: Option<Instant>,
    memory: Option<[PDH_HCOUNTER; 2]>,
    // Native structures and their UTF-16 names share this aligned, bounded buffer.
    buffer: Vec<u64>,
}
impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.handle);
        }
    }
}
impl Query {
    pub(super) fn open() -> Result<Self, Failure> {
        let mut handle = ptr::null_mut();
        check("open", unsafe {
            PdhOpenQueryW(ptr::null(), 0, &mut handle)
        })?;
        let mut query = Self {
            handle,
            counter: ptr::null_mut(),
            previous: None,
            memory: None,
            buffer: Vec::new(),
        };
        check("add", unsafe {
            PdhAddEnglishCounterW(
                handle,
                w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut query.counter,
            )
        })?;
        Ok(query)
    }

    pub(super) fn read(&mut self) -> Result<Option<HashMap<Engine, f64>>, Failure> {
        check("collect", unsafe { PdhCollectQueryData(self.handle) })?;
        let now = Instant::now();
        let interval = self.previous.replace(now).map(|at| now.duration_since(at));
        if !interval.is_some_and(|interval| (100..=5000).contains(&(interval.as_millis() as u64))) {
            return Ok(None);
        }
        let values = self.values(self.counter)?;
        aggregate(values).map(Some)
    }

    pub(super) fn set_memory(&mut self, enabled: bool) -> Result<(), Failure> {
        if !enabled {
            if let Some(counters) = self.memory.take() {
                for counter in counters {
                    unsafe {
                        PdhRemoveCounter(counter);
                    }
                }
            }
            return Ok(());
        }
        if self.memory.is_some() {
            return Ok(());
        }
        let mut counters = [ptr::null_mut(); 2];
        for (index, path) in [
            w!("\\GPU Adapter Memory(*)\\Dedicated Usage"),
            w!("\\GPU Adapter Memory(*)\\Shared Usage"),
        ]
        .into_iter()
        .enumerate()
        {
            let code = unsafe { PdhAddEnglishCounterW(self.handle, path, 0, &mut counters[index]) };
            if code != 0 {
                for counter in &counters[..index] {
                    unsafe {
                        PdhRemoveCounter(*counter);
                    }
                }
                return Err(Failure {
                    stage: "memory_add",
                    code,
                });
            }
        }
        self.memory = Some(counters);
        Ok(())
    }

    pub(super) fn memory(&mut self) -> Result<[Vec<(String, f64)>; 2], Failure> {
        let counters = self.memory.ok_or(Failure {
            stage: "memory_missing",
            code: PDH_CSTATUS_NO_COUNTER,
        })?;
        Ok([self.values(counters[0])?, self.values(counters[1])?])
    }

    pub(super) fn values(&mut self, counter: PDH_HCOUNTER) -> Result<Vec<(String, f64)>, Failure> {
        // A process may appear between size and data calls. Retry a bounded number
        // of times, and query size afresh instead of trusting an undersized call.
        for _ in 0..3 {
            let mut size = (self.buffer.len() * mem::size_of::<u64>()) as u32;
            let mut count = 0;
            let raw = if size == 0 {
                ptr::null_mut()
            } else {
                self.buffer
                    .as_mut_ptr()
                    .cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()
            };
            let code = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_DOUBLE | PDH_FMT_NOCAP100,
                    &mut size,
                    &mut count,
                    raw,
                )
            };
            if code == PDH_MORE_DATA {
                size = 0;
                let code = unsafe {
                    PdhGetFormattedCounterArrayW(
                        counter,
                        PDH_FMT_DOUBLE | PDH_FMT_NOCAP100,
                        &mut size,
                        &mut count,
                        ptr::null_mut(),
                    )
                };
                if code != PDH_MORE_DATA {
                    return Err(Failure {
                        stage: "size",
                        code,
                    });
                }
                if size as usize > MAX_BUFFER_BYTES || size == 0 {
                    return Err(Failure {
                        stage: "buffer_limit",
                        code: PDH_INVALID_DATA,
                    });
                }
                self.buffer
                    .resize((size as usize).div_ceil(mem::size_of::<u64>()), 0);
                continue;
            }
            check("format", code)?;
            return decode_values(&self.buffer, size as usize, count);
        }
        Err(Failure {
            stage: "resize",
            code: PDH_MORE_DATA,
        })
    }
}

// Validate native pointers before interpreting the query-owned buffer.
fn decode_values(buffer: &[u64], bytes: usize, count: u32) -> Result<Vec<(String, f64)>, Failure> {
    if bytes > buffer.len() * 8
        || count as usize > bytes / mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()
    {
        return Err(Failure {
            stage: "bounds",
            code: PDH_INVALID_DATA,
        });
    }
    let start = buffer.as_ptr() as usize;
    let end = start + bytes;
    let items = if count == 0 {
        &[][..]
    } else {
        unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                count as usize,
            )
        }
    };
    let mut values = Vec::new();
    for item in items {
        if !matches!(
            item.FmtValue.CStatus,
            PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
        ) {
            continue;
        }
        let address = item.szName as usize;
        if address < start || address >= end || !address.is_multiple_of(mem::align_of::<u16>()) {
            return Err(Failure {
                stage: "name_bounds",
                code: PDH_INVALID_DATA,
            });
        }
        let units = unsafe { std::slice::from_raw_parts(item.szName, (end - address) / 2) };
        let length = units
            .iter()
            .take(1024)
            .position(|unit| *unit == 0)
            .ok_or(Failure {
                stage: "name_length",
                code: PDH_INVALID_DATA,
            })?;
        let name = String::from_utf16(&units[..length]).map_err(|_| Failure {
            stage: "name_utf16",
            code: PDH_INVALID_DATA,
        })?;
        let value = unsafe { item.FmtValue.Anonymous.doubleValue };
        if !value.is_finite() || value < 0.0 {
            continue;
        }
        values.push((name, value));
    }
    if values.is_empty() {
        return Err(Failure {
            stage: "empty",
            code: PDH_NO_DATA,
        });
    }
    Ok(values)
}

fn aggregate(values: Vec<(String, f64)>) -> Result<HashMap<Engine, f64>, Failure> {
    let mut engines = HashMap::new();
    for (name, value) in values {
        if let Some(key) = engine(&name) {
            *engines.entry(key).or_default() += value;
        }
    }
    if engines.is_empty() {
        return Err(Failure {
            stage: "empty",
            code: PDH_NO_DATA,
        });
    }
    Ok(engines)
}

#[cfg(test)]
pub(super) fn decode(
    buffer: &[u64],
    bytes: usize,
    count: u32,
) -> Result<HashMap<Engine, f64>, Failure> {
    aggregate(decode_values(buffer, bytes, count)?)
}
