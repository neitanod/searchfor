//! One file's output while other threads search (M5b spec §6.2): kept in memory up to
//! `FILE_SPOOL_THRESHOLD` bytes, and past it in an unnamed temporary file, which the system
//! deletes when it is closed or when s4 dies, so a crash between the spill and the copy leaves
//! nothing on disk. The output is copied to stdout once the file ends, so files never
//! interleave. Only bytes are spooled: plain and ANSI output, never a Windows console buffer,
//! whose colours live beside its bytes (plan D12).
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Bytes a file's output keeps in memory: up to 8 MiB, and the next byte spills it.
pub const FILE_SPOOL_THRESHOLD: usize = 8 * 1024 * 1024;

/// The buffer of the temporary file's writes, and of its copy.
const CHUNK: usize = 64 * 1024;

pub struct FileSpool {
    dir: PathBuf,
    /// The output while it fits; empty, with no capacity, once spilled.
    memory: Vec<u8>,
    /// The temporary file once the output passed the threshold: the memory prefix was written
    /// to it once and released, so the output is never held twice.
    file: Option<BufWriter<File>>,
}

impl FileSpool {
    /// A spool whose temporary file, made only when needed, goes in `dir`.
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        Ok(Self {
            dir: dir.to_path_buf(),
            memory: Vec::new(),
            file: None,
        })
    }

    pub fn is_spilled(&self) -> bool {
        self.file.is_some()
    }

    /// The bytes held in memory.
    pub fn memory_len(&self) -> usize {
        self.memory.len()
    }

    /// Nothing was written since the last `clear`.
    pub fn is_empty(&self) -> bool {
        self.memory.is_empty() && self.file.is_none()
    }

    /// The bytes held in memory, when the output was not spilled.
    pub fn in_memory(&self) -> Option<&[u8]> {
        self.file.is_none().then_some(&self.memory[..])
    }

    /// Writes everything spooled to `out`, from the start, in bounded pieces; returns how many
    /// bytes. The spool keeps its contents until `clear`.
    pub fn copy_to(&mut self, out: &mut dyn Write) -> std::io::Result<u64> {
        self.each_piece(&mut |piece| out.write_all(piece))
    }

    /// Calls `f` with everything spooled, from the start, in pieces of at most 64 KiB
    /// (`CHUNK`) that end at a line break, except a longer line, given whole, and the end.
    /// A piece never splits a line, so a writer that converts text per piece (a Windows
    /// console) never sees half a character. Returns how many bytes.
    pub fn each_piece(
        &mut self,
        f: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<u64> {
        let Some(file) = self.file.as_mut() else {
            if !self.memory.is_empty() {
                f(&self.memory)?;
            }
            return Ok(self.memory.len() as u64);
        };
        file.flush()?;
        let file = file.get_mut();
        file.seek(SeekFrom::Start(0))?;
        let mut held: Vec<u8> = Vec::with_capacity(CHUNK);
        let mut total = 0u64;
        let mut scratch = vec![0u8; CHUNK];
        loop {
            let n = match file.read(&mut scratch) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            held.extend_from_slice(&scratch[..n]);
            if held.len() >= CHUNK
                && let Some(end) = memchr::memrchr(b'\n', &held)
            {
                f(&held[..=end])?;
                total += end as u64 + 1;
                held.drain(..=end);
                // A long line grew it: give the memory back.
                held.shrink_to(CHUNK);
            }
        }
        if !held.is_empty() {
            f(&held)?;
            total += held.len() as u64;
        }
        // The file is written on after its end, if anything is written again.
        file.seek(SeekFrom::End(0))?;
        Ok(total)
    }

    /// Empties the spool for the next file: the temporary file is closed, which deletes it,
    /// and the memory keeps its capacity, at most the threshold.
    pub fn clear(&mut self) {
        self.memory.clear();
        self.file = None;
    }

    fn spill(&mut self) -> std::io::Result<&mut BufWriter<File>> {
        if self.file.is_none() {
            let mut f = BufWriter::with_capacity(CHUNK, tempfile::tempfile_in(&self.dir)?);
            f.write_all(&self.memory)?;
            self.memory = Vec::new();
            self.file = Some(f);
        }
        Ok(self.file.as_mut().expect("spilled above"))
    }
}

impl Write for FileSpool {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.file.is_none() && self.memory.len() + bytes.len() <= FILE_SPOOL_THRESHOLD {
            self.memory.extend_from_slice(bytes);
            return Ok(bytes.len());
        }
        self.spill()?.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn spills_after_eight_mib_and_releases_the_memory_prefix() {
        let d = tempfile::tempdir().unwrap();
        let mut s = FileSpool::new(d.path()).unwrap();
        s.write_all(&vec![b'a'; FILE_SPOOL_THRESHOLD]).unwrap();
        assert!(!s.is_spilled());
        s.write_all(b"z").unwrap();
        assert!(s.is_spilled());
        assert_eq!(s.memory_len(), 0);
        let mut out = Vec::new();
        assert_eq!(
            s.copy_to(&mut out).unwrap(),
            FILE_SPOOL_THRESHOLD as u64 + 1
        );
        assert!(out[..FILE_SPOOL_THRESHOLD].iter().all(|&b| b == b'a'));
        assert_eq!(out[FILE_SPOOL_THRESHOLD], b'z');
        drop(s);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }
    #[test]
    fn one_large_write_never_duplicates_the_whole_output_in_ram() {
        let d = tempfile::tempdir().unwrap();
        let mut s = FileSpool::new(d.path()).unwrap();
        s.write_all(&vec![b'x'; FILE_SPOOL_THRESHOLD * 2]).unwrap();
        assert!(s.is_spilled());
        assert_eq!(s.memory_len(), 0);
        let mut count = std::io::sink();
        assert_eq!(
            s.copy_to(&mut count).unwrap(),
            (FILE_SPOOL_THRESHOLD * 2) as u64
        );
    }

    #[test]
    fn pieces_end_at_line_breaks_and_clear_starts_over() {
        let d = tempfile::tempdir().unwrap();
        let mut s = FileSpool::new(d.path()).unwrap();
        let line = b"0123456789abcdef0123456789abcdef0123456789abcdef012345678\n";
        let lines = FILE_SPOOL_THRESHOLD / line.len() + 10;
        for _ in 0..lines {
            s.write_all(line).unwrap();
        }
        s.write_all(b"no end").unwrap();
        assert!(s.is_spilled());
        let mut pieces = Vec::new();
        let total = s
            .each_piece(&mut |p| {
                pieces.push(p.to_vec());
                Ok(())
            })
            .unwrap();
        assert_eq!(total, (lines * line.len() + 6) as u64);
        let (last, whole) = pieces.split_last().unwrap();
        assert!(
            whole
                .iter()
                .all(|p| p.ends_with(b"\n") && p.len() <= 2 * CHUNK)
        );
        assert!(last.ends_with(b"no end"));
        // Copied again, the same bytes.
        let mut again = Vec::new();
        assert_eq!(s.copy_to(&mut again).unwrap(), total);
        assert_eq!(again, pieces.concat());
        s.clear();
        assert!(s.is_empty() && !s.is_spilled());
        s.write_all(b"next\n").unwrap();
        assert_eq!(s.in_memory(), Some(&b"next\n"[..]));
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }
}
