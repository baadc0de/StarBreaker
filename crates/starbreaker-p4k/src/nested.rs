//! Read a member of a ZIP stored uncompressed inside a P4k (e.g. a shader cache).
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use crate::archive::P4kArchive;
use crate::error::P4kError;
use crate::owned::MappedP4k;

impl MappedP4k {
    /// Read one member without copying the enclosing archive to memory or disk.
    pub fn read_stored_archive_file(&self, archive: &str, path: &str) -> Result<Vec<u8>, P4kError> {
        let outer = self
            .entry_case_insensitive(archive)
            .ok_or_else(|| P4kError::EntryNotFound(archive.to_owned()))?;
        if outer.is_encrypted {
            return Err(P4kError::EncryptedNonZstd(outer.compression_method));
        }
        if outer.compression_method != 0 {
            return Err(P4kError::UnsupportedCompression(outer.compression_method));
        }
        let mut file = File::open(self.path())?;
        let start = P4kArchive::file_data_offset(&file, outer)?;
        let file_size = file.metadata()?.len();
        if start
            .checked_add(outer.uncompressed_size)
            .is_none_or(|end| end > file_size)
        {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "nested archive exceeds file").into(),
            );
        }
        let window = ArchiveWindow {
            file: &mut file,
            start,
            len: outer.uncompressed_size,
            pos: 0,
        };
        let mut zip = zip::ZipArchive::new(window).map_err(zip_error)?;
        let mut member = zip.by_name(path).map_err(zip_error)?;
        let mut data = Vec::new();
        member.read_to_end(&mut data)?;
        Ok(data)
    }
}

fn zip_error(error: zip::result::ZipError) -> P4kError {
    io::Error::new(io::ErrorKind::InvalidData, error).into()
}
/// A seekable file window with all ZIP offsets relative to its first byte.
struct ArchiveWindow<'a> {
    file: &'a mut File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for ArchiveWindow<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let limit = self.len.saturating_sub(self.pos).min(buf.len() as u64) as usize;
        if limit == 0 {
            return Ok(0);
        }
        let count = self.file.read(&mut buf[..limit])?;
        self.pos += count as u64;
        Ok(count)
    }
}

impl Seek for ArchiveWindow<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(offset) => self.len.checked_add_signed(offset),
            SeekFrom::Current(offset) => self.pos.checked_add_signed(offset),
        }
        .filter(|&pos| pos <= self.len)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek outside nested archive")
        })?;
        self.file.seek(SeekFrom::Start(self.start + pos))?;
        self.pos = pos;
        Ok(pos)
    }
}
