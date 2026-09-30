//! Pack-specific bounded ZIP preflight. Snapshot's accepted profile is unchanged.
//! The record checker follows the existing audited Snapshot checker, but retains
//! compressed extents and validates general safe member names for Pack containers.
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};
#[derive(Debug)]
pub(crate) enum BundleError {
    Profile(&'static str),
    Io(io::ErrorKind),
}
impl From<io::Error> for BundleError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile(s) => write!(f, "Pack ZIP: {s}"),
            Self::Io(k) => write!(f, "Pack ZIP I/O: {k}"),
        }
    }
}
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) compressed: u64,
    pub(crate) method: u16,
    pub(crate) crc: u32,
    flags: u16,
    header: u64,
    pub(crate) data: u64,
}
fn invalid<T>(s: &'static str) -> Result<T, BundleError> {
    Err(BundleError::Profile(s))
}
fn check_limit(value: u64, maximum: u64) -> Result<(), BundleError> {
    if value > maximum {
        invalid("Pack structural resource limit exceeded")
    } else {
        Ok(())
    }
}
fn add(a: u64, b: u64) -> Result<u64, BundleError> {
    a.checked_add(b)
        .ok_or(BundleError::Profile("ZIP offset overflow"))
}
fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn u64le(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap())
}
fn at<const N: usize>(file: &mut File, offset: u64, end: u64) -> Result<[u8; N], BundleError> {
    if add(offset, N as u64)? > end {
        return invalid("ZIP record outside its section");
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut b = [0; N];
    file.read_exact(&mut b)?;
    Ok(b)
}
fn variable(file: &mut File, offset: u64, len: usize, end: u64) -> Result<Vec<u8>, BundleError> {
    if add(offset, len as u64)? > end {
        return invalid("ZIP variable record outside its section");
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut b = vec![0; len];
    file.read_exact(&mut b)?;
    Ok(b)
}
fn zip64(
    extra: &[u8],
    size: u32,
    compressed: u32,
    offset: u32,
    disk: u16,
) -> Result<(u64, u64, u64, u32), BundleError> {
    let mut cursor = 0;
    let mut record = None;
    while cursor < extra.len() {
        if extra.len() - cursor < 4 {
            return invalid("truncated ZIP extra field");
        };
        let tag = u16le(extra, cursor);
        let length = u16le(extra, cursor + 2) as usize;
        cursor += 4;
        if length > extra.len() - cursor {
            return invalid("truncated ZIP extra payload");
        };
        if tag == 1 {
            if record.is_some() {
                return invalid("duplicate ZIP64 field");
            };
            record = Some(&extra[cursor..cursor + length]);
        }
        if matches!(tag, 0x0017 | 0x9901) {
            return invalid("encrypted ZIP extra field");
        };
        cursor += length;
    }
    let needs =
        size == u32::MAX || compressed == u32::MAX || offset == u32::MAX || disk == u16::MAX;
    if !needs {
        return Ok((size.into(), compressed.into(), offset.into(), disk.into()));
    };
    let b = record.ok_or(BundleError::Profile("missing ZIP64 extra field"))?;
    let mut p = 0;
    let mut wide = || -> Result<u64, BundleError> {
        if b.len() - p < 8 {
            return invalid("truncated ZIP64 value");
        };
        let v = u64le(b, p);
        p += 8;
        Ok(v)
    };
    let size = if size == u32::MAX {
        wide()?
    } else {
        size.into()
    };
    let compressed = if compressed == u32::MAX {
        wide()?
    } else {
        compressed.into()
    };
    let offset = if offset == u32::MAX {
        wide()?
    } else {
        offset.into()
    };
    let disk = if disk == u16::MAX {
        if b.len() - p < 4 {
            return invalid("truncated ZIP64 disk");
        };
        u32le(b, p)
    } else {
        disk.into()
    };
    Ok((size, compressed, offset, disk))
}
fn member_name(bytes: &[u8]) -> Result<String, BundleError> {
    let name =
        std::str::from_utf8(bytes).map_err(|_| BundleError::Profile("ZIP names must be UTF-8"))?;
    crate::authoring::SourceRelativePathV1::parse(name.strip_suffix('/').unwrap_or(name))
        .map_err(|_| BundleError::Profile("unsafe ZIP member path"))?;
    Ok(name.to_owned())
}
pub(crate) fn preflight(file: &mut File) -> Result<(Vec<Entry>, u64), BundleError> {
    let length = file.metadata()?.len();
    if length < 22 {
        return invalid("missing ZIP end record");
    };
    let tail_size = length.min(65_557) as usize;
    let start = length - tail_size as u64;
    let tail = variable(file, start, tail_size, length)?;
    let position = (0..=tail.len() - 22)
        .rev()
        .find(|&p| {
            u32le(&tail, p) == 0x06054b50 && p + 22 + u16le(&tail, p + 20) as usize == tail.len()
        })
        .ok_or(BundleError::Profile("invalid ZIP end record"))?;
    let end_record = start + position as u64;
    let e = &tail[position..];
    if u16le(e, 4) != 0 || u16le(e, 6) != 0 || u16le(e, 8) != u16le(e, 10) {
        return invalid("multi-volume ZIP is not supported");
    };
    let (mut count, mut directory_size, mut directory) = (
        u16le(e, 10) as u64,
        u32le(e, 12) as u64,
        u32le(e, 16) as u64,
    );
    let mut directory_end = end_record;
    let locator = if end_record >= 20 {
        Some(at::<20>(file, end_record - 20, length)?)
    } else {
        None
    };
    if let Some(locator) = locator.filter(|b| u32le(b, 0) == 0x07064b50) {
        if u32le(&locator, 4) != 0 || u32le(&locator, 16) != 1 {
            return invalid("multi-volume ZIP64");
        };
        let offset = u64le(&locator, 8);
        let z = at::<56>(file, offset, end_record - 20)?;
        let record_size = u64le(&z, 4);
        if u32le(&z, 0) != 0x06064b50
            || record_size < 44
            || add(offset, add(12, record_size)?)? != end_record - 20
            || u32le(&z, 16) != 0
            || u32le(&z, 20) != 0
            || u64le(&z, 24) != u64le(&z, 32)
        {
            return invalid("invalid ZIP64 end record");
        };
        let (c, s, o) = (u64le(&z, 32), u64le(&z, 40), u64le(&z, 48));
        if (count != u16::MAX as u64 && count != c)
            || (directory_size != u32::MAX as u64 && directory_size != s)
            || (directory != u32::MAX as u64 && directory != o)
        {
            return invalid("inconsistent ZIP64 directory");
        };
        (count, directory_size, directory) = (c, s, o);
        directory_end = offset;
    } else if count == u16::MAX as u64
        || directory_size == u32::MAX as u64
        || directory == u32::MAX as u64
    {
        return invalid("missing ZIP64 locator");
    };
    check_limit(count, 65_539)?;
    if count == 0 || add(directory, directory_size)? != directory_end || directory_end > length {
        return invalid("invalid central directory extent");
    };
    let mut metadata = length - directory;
    check_limit(metadata, 64 * 1024 * 1024)?;
    let mut entries = Vec::with_capacity(count as usize);
    let mut names = BTreeSet::new();
    let mut cursor = directory;

    for _ in 0..count {
        let h = at::<46>(file, cursor, directory_end)?;
        if u32le(&h, 0) != 0x02014b50
            || !matches!(u16le(&h, 10), 0 | 8)
            || u16le(&h, 8) & !0x080e != 0
        {
            return invalid("unsupported central ZIP record");
        };
        let (name_len, extra_len, comment_len) = (
            u16le(&h, 28) as usize,
            u16le(&h, 30) as usize,
            u16le(&h, 32) as usize,
        );
        if name_len > 1025 {
            return invalid("invalid ZIP member name");
        };
        let name_bytes = variable(file, add(cursor, 46)?, name_len, directory_end)?;
        let name = member_name(&name_bytes)?;
        if !names.insert(name.clone()) {
            return invalid("duplicate ZIP member");
        };
        let extra = variable(
            file,
            add(cursor, 46 + name_len as u64)?,
            extra_len,
            directory_end,
        )?;
        let (size, compressed, header, disk) = zip64(
            &extra,
            u32le(&h, 24),
            u32le(&h, 20),
            u32le(&h, 42),
            u16le(&h, 34),
        )?;
        if disk != 0 || (u16le(&h, 10) == 0 && size != compressed) {
            return invalid("non-Stored or split ZIP member");
        };
        let attr = u32le(&h, 38);
        let kind = (attr >> 16) & 0xf000;
        if attr & 8 != 0
            || !matches!(kind, 0 | 0x8000 | 0x4000)
            || ((kind == 0x4000 || attr & 0x10 != 0) && !name.ends_with('/'))
            || (name.ends_with('/') && size != 0)
        {
            return invalid("non-regular ZIP member");
        };
        entries.push(Entry {
            name,
            size,
            compressed,
            method: u16le(&h, 10),
            crc: u32le(&h, 16),
            flags: u16le(&h, 8),
            header,
            data: 0,
        });
        cursor = add(
            cursor,
            46 + name_len as u64 + extra_len as u64 + comment_len as u64,
        )?;
        if cursor > directory_end {
            return invalid("central ZIP member exceeds directory");
        };
    }
    if cursor != directory_end {
        return invalid("incomplete central directory");
    };
    let mut order: Vec<_> = (0..entries.len()).collect();
    order.sort_by_key(|&i| entries[i].header);
    let mut next = 0;
    for i in order {
        let entry = &mut entries[i];
        if entry.header != next {
            return invalid("ZIP prefix, overlap, hidden entry, or gap");
        };
        let h = at::<30>(file, entry.header, directory)?;
        if u32le(&h, 0) != 0x04034b50 || u16le(&h, 6) != entry.flags || u16le(&h, 8) != entry.method
        {
            return invalid("local/central ZIP disagreement");
        };
        let (n, x) = (u16le(&h, 26) as usize, u16le(&h, 28) as usize);
        if n != entry.name.len() {
            return invalid("local ZIP name mismatch");
        };
        if variable(file, add(entry.header, 30)?, n, directory)? != entry.name.as_bytes() {
            return invalid("local ZIP name mismatch");
        };
        let extra = variable(file, add(entry.header, 30 + n as u64)?, x, directory)?;
        let (s, c, _, _) = zip64(&extra, u32le(&h, 22), u32le(&h, 18), 0, 0)?;
        let descriptor = entry.flags & 8 != 0;
        if (!descriptor && (s != entry.size || c != entry.compressed || u32le(&h, 14) != entry.crc))
            || (descriptor
                && ((s != 0 && s != entry.size)
                    || (c != 0 && c != entry.compressed)
                    || (u32le(&h, 14) != 0 && u32le(&h, 14) != entry.crc)))
        {
            return invalid("local ZIP sizes or CRC disagree");
        };
        metadata = add(metadata, 30 + n as u64 + x as u64)?;
        check_limit(metadata, 64 * 1024 * 1024)?;
        entry.data = add(entry.header, 30 + n as u64 + x as u64)?;
        next = add(entry.data, entry.compressed)?;
        if next > directory {
            return invalid("ZIP payload extends into metadata");
        };
        if descriptor {
            let wide = u32le(&h, 18) == u32::MAX
                || u32le(&h, 22) == u32::MAX
                || entry.size > u32::MAX as u64;
            let total = descriptor_length(
                file,
                next,
                directory,
                wide,
                entry.crc,
                entry.size,
                entry.compressed,
            )?;
            metadata = add(metadata, total)?;
            check_limit(metadata, 64 * 1024 * 1024)?;
            next = add(next, total)?;
        }
    }
    if next != directory {
        return invalid("unaccounted local ZIP data");
    };
    Ok((entries, directory))
}

fn descriptor_length(
    file: &mut File,
    offset: u64,
    end: u64,
    wide: bool,
    crc: u32,
    length: u64,
    compressed: u64,
) -> Result<u64, BundleError> {
    let first = at::<4>(file, offset, end)?;
    for signed in [true, false] {
        if signed && u32le(&first, 0) != 0x08074b50 {
            continue;
        };
        let prefix = if signed { 4 } else { 0 };
        let body_size = if wide { 20 } else { 12 };
        let total = prefix + body_size;
        if add(offset, total as u64)? > end {
            continue;
        };
        let b = variable(file, offset + prefix as u64, body_size, end)?;
        let (a, c) = if wide {
            (u64le(&b, 4), u64le(&b, 12))
        } else {
            (u32le(&b, 4) as u64, u32le(&b, 8) as u64)
        };
        if u32le(&b, 0) == crc && a == compressed && c == length {
            return Ok(total as u64);
        }
    }
    invalid("invalid ZIP data descriptor")
}
