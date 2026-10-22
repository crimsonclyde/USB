//! Bounded streaming; generic readers/writers permit meaningful fault injection.
use crate::{Error, Result, cancelled, fs::io};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::atomic::AtomicBool,
};

pub(crate) const BUFFER_BYTES: usize = 128 * 1024;

pub(crate) fn copy_stream<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected: u64,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, usize) -> Result<()>,
) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut copied = 0;
    while copied < expected {
        cancelled(cancel)?;
        let count = usize::try_from((expected - copied).min(BUFFER_BYTES as u64))
            .map_err(|_| Error::Overflow)?;
        let read = loop {
            match reader.read(&mut buffer[..count]) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                result => break io("read source bytes", "payload", result)?,
            }
        };
        if read == 0 {
            return Err(Error::SourceChanged("payload shrank".into()));
        }
        // The callback with nonzero upcoming size checks free space before writing.
        progress(copied, read)?;
        cancelled(cancel)?;
        io(
            "write payload bytes",
            "staging",
            writer.write_all(&buffer[..read]),
        )?;
        digest.update(&buffer[..read]);
        copied += read as u64;
        progress(copied, 0)?;
    }
    cancelled(cancel)?;
    let mut extra = [0_u8];
    io(
        "check source length",
        "payload",
        reader.read_exact(&mut extra),
    )
    .map_or_else(
        |error| match error {
            Error::Io { source, .. } if source.kind() == std::io::ErrorKind::UnexpectedEof => {
                Ok(())
            }
            error => Err(error),
        },
        |_| Err(Error::SourceChanged("payload grew".into())),
    )?;
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub(crate) fn hash(reader: &mut impl Read, expected: u64, cancel: &AtomicBool) -> Result<String> {
    copy_stream(
        reader,
        &mut std::io::sink(),
        expected,
        cancel,
        |_, _| Ok(()),
    )
}
