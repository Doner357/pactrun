//! Shared physical representation for chunked immutable blobs.
//!
//! Managed Input payloads and Run Artifacts store bytes as one header row plus
//! contiguous one-megabyte chunks. This module owns the cross-row invariants
//! that SQLite row checks cannot express: contiguous indices from zero, every
//! non-final chunk full, and aggregate length equal to the header.

use std::io::{Read, Write};

use rusqlite::{Connection, Transaction, params};

use super::PersistenceError;
use crate::domain::MANAGED_INPUT_CHUNK_BYTES_V1;

/// The SQL and error mapping of one chunk table keyed by two BLOB columns.
pub(super) struct ChunkedBlobTable {
    pub(super) representation_maximum: u64,
    /// `INSERT ... VALUES (?1, ?2, ?3, ?4)` binding key0, key1, chunk_index,
    /// chunk_bytes.
    pub(super) insert_chunk_sql: &'static str,
    /// `SELECT chunk_index, chunk_bytes ... WHERE key0 = ?1 AND key1 = ?2
    /// ORDER BY chunk_index`.
    pub(super) select_chunks_sql: &'static str,
    pub(super) read_operation: &'static str,
    pub(super) write_operation: &'static str,
    pub(super) invalid: fn(String) -> PersistenceError,
    pub(super) corrupt: fn(String) -> PersistenceError,
}

pub(super) fn insert_chunks(
    transaction: &Transaction<'_>,
    table: &ChunkedBlobTable,
    key: [&[u8]; 2],
    reader: &mut dyn Read,
    byte_len: u64,
) -> Result<(), PersistenceError> {
    if byte_len > table.representation_maximum {
        return Err((table.invalid)("blob exceeds the size limit".to_owned()));
    }
    let mut total = 0_u64;
    let mut chunk_index = 0_i64;
    loop {
        let mut chunk = vec![0_u8; MANAGED_INPUT_CHUNK_BYTES_V1];
        let mut used = 0;
        while used < chunk.len() {
            let read = reader
                .read(&mut chunk[used..])
                .map_err(|source| PersistenceError::Io {
                    operation: table.read_operation,
                    source,
                })?;
            if read == 0 {
                break;
            }
            used += read;
        }
        if used == 0 {
            break;
        }
        chunk.truncate(used);
        total = total
            .checked_add(u64::try_from(used).expect("chunk length fits u64"))
            .ok_or_else(|| (table.invalid)("blob length overflow".to_owned()))?;
        if total > byte_len || total > table.representation_maximum {
            return Err((table.invalid)("staged blob length changed".to_owned()));
        }
        transaction
            .execute(
                table.insert_chunk_sql,
                params![key[0], key[1], chunk_index, chunk],
            )
            .map_err(|error| PersistenceError::sqlite(table.write_operation, error))?;
        chunk_index += 1;
    }
    if total != byte_len {
        return Err((table.invalid)("staged blob length changed".to_owned()));
    }
    Ok(())
}

pub(super) fn stream_chunks(
    database: &Connection,
    table: &ChunkedBlobTable,
    key: [&[u8]; 2],
    declared_length: u64,
    destination: &mut dyn Write,
) -> Result<(), PersistenceError> {
    if declared_length > table.representation_maximum {
        return Err((table.corrupt)("oversize blob header".to_owned()));
    }
    let mut statement = database
        .prepare(table.select_chunks_sql)
        .map_err(|error| PersistenceError::sqlite(table.read_operation, error))?;
    let rows = statement
        .query_map(params![key[0], key[1]], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| PersistenceError::sqlite(table.read_operation, error))?;
    let mut expected_index = 0_i64;
    let mut total = 0_u64;
    let mut previous_length = None;
    for row in rows {
        let (index, bytes) =
            row.map_err(|error| PersistenceError::sqlite(table.read_operation, error))?;
        if index != expected_index
            || bytes.is_empty()
            || bytes.len() > MANAGED_INPUT_CHUNK_BYTES_V1
            || previous_length.is_some_and(|length| length != MANAGED_INPUT_CHUNK_BYTES_V1)
        {
            return Err((table.corrupt)("invalid chunk sequence".to_owned()));
        }
        total = total
            .checked_add(u64::try_from(bytes.len()).expect("chunk length fits u64"))
            .ok_or_else(|| (table.corrupt)("blob length overflow".to_owned()))?;
        if total > declared_length {
            return Err((table.corrupt)("chunk length exceeds header".to_owned()));
        }
        destination
            .write_all(&bytes)
            .map_err(|source| PersistenceError::Io {
                operation: table.write_operation,
                source,
            })?;
        previous_length = Some(bytes.len());
        expected_index += 1;
    }
    if total != declared_length || (declared_length == 0 && expected_index != 0) {
        return Err((table.corrupt)("blob length mismatch".to_owned()));
    }
    Ok(())
}
