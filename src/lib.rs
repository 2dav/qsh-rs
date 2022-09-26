use flate2::bufread::GzDecoder;
use std::{
    fs::File,
    io::{BufRead, BufReader, ErrorKind, Read},
    path::PathBuf,
};
use thiserror::Error;
pub mod orderbook;
mod parse;
pub mod types;
pub mod utils;
pub use parse::{AuxInfoReader, DealReader, OrderLogReader, QshParser, QuotesReader};

use crate::types::Header;
use leb128::read as leb128;

#[derive(Error, Debug)]
pub enum QshError {
    #[error("")]
    IO {
        #[from]
        source: std::io::Error,
    },
    #[error("")]
    General {
        #[from]
        source: Box<dyn std::error::Error>,
    },
    #[error("Validation error: `{0}`")]
    Validation(String),
    #[error("Invalid internal state: `{0}`")]
    InvalidState(String),
    #[error("QSH parsing error: `{0}`")]
    Parsing(String),
}

unsafe impl Send for QshError {}
unsafe impl Sync for QshError {}

pub fn inflate(path: PathBuf) -> Result<impl BufRead, QshError> {
    File::open(path)
        .map(BufReader::new)
        .map(GzDecoder::new)
        .map(BufReader::new)
        .map_err(|err| err.into())
}

pub trait QshRead: Read + Sized {
    fn byte(&mut self) -> Result<u8, QshError> {
        self.consume_with(1, |b| b[0])
    }

    fn i64(&mut self) -> Result<i64, QshError> {
        self.u64().map(|u| u as i64)
    }

    fn u64(&mut self) -> Result<u64, QshError> {
        self.consume_with(8, |buf| {
            let mut res = 0u64;
            for (index, &byte) in buf.iter().enumerate() {
                res += (byte as u64) << (8 * index);
            }
            res
        })
    }

    fn f64(&mut self) -> Result<f64, QshError> {
        self.u64().map(f64::from_bits)
    }

    fn i16(&mut self) -> Result<i16, QshError> {
        self.u16().map(|u| u as i16)
    }

    fn u16(&mut self) -> Result<u16, QshError> {
        self.consume_with(2, |buf| {
            let mut res = 0u16;
            for (index, &byte) in buf.iter().enumerate() {
                res += (byte as u16) << (8 * index);
            }
            res
        })
    }

    fn uleb(&mut self) -> Result<u64, QshError> {
        leb128::unsigned(self).map_err(|err| QshError::General { source: Box::new(err) })
    }

    fn leb(&mut self) -> Result<i64, QshError> {
        leb128::signed(self).map_err(|err| QshError::General { source: Box::new(err) })
    }

    fn growing(&mut self) -> Result<i64, QshError> {
        match self.uleb()? {
            268_435_455 => self.leb(),
            x => Ok(x as i64),
        }
    }

    fn string(&mut self) -> Result<String, QshError> {
        self.leb().and_then(|n| {
            self.consume_with(n as usize, |buf| {
                String::from_utf8(buf.to_vec())
                    .map_err(|err| QshError::General { source: Box::new(err) })
            })
        })?
    }

    fn into_iter<T: QshParser>(self) -> RecordIter<T, Self> {
        RecordIter(T::default(), self)
    }

    fn consume_with<F, T>(&mut self, n: usize, f: F) -> Result<T, QshError>
    where
        F: Fn(&[u8]) -> T;

    fn eof(&mut self) -> Result<bool, QshError>;
}

impl<T> QshRead for T
where
    T: BufRead,
{
    fn consume_with<F, U>(&mut self, n: usize, f: F) -> Result<U, QshError>
    where
        F: Fn(&[u8]) -> U,
    {
        debug_assert!(n > 0, "requested byte slice size should be > 0");

        let buf = self.fill_buf()?;

        match buf.len() {
            x if x >= n => {
                let ret = f(&buf[..n]);
                self.consume(n);
                Ok(ret)
            }
            0 => Err(QshError::IO { source: ErrorKind::UnexpectedEof.into() }),
            x => {
                // workaround for buffer boundary
                // [..x][x..]
                // if the requested chunk happens to cross 'BufReader's internal buffer boundary,
                // 'fill_buff' won't fill the buffer with the remaining bytes until the current buffer
                // is non-empty. This happens from time to time as the qsh format defines fields of arbitrary length.
                let mut b = Vec::with_capacity(n);
                b.extend_from_slice(buf);

                self.consume(x);

                let rem = n - x;
                let buf = self.fill_buf()?;

                if buf.len() < rem {
                    Err(QshError::IO { source: ErrorKind::UnexpectedEof.into() })
                } else {
                    b.extend_from_slice(&buf[..rem]);
                    let ret = f(&b[..]);
                    self.consume(rem);
                    Ok(ret)
                }
                // technically, this still fails on 'n' >= buffer capacity(8 << 20), though we
                // don't use such chunks anywhere
            }
        }
    }

    fn eof(&mut self) -> Result<bool, QshError> {
        self.fill_buf().map(|b| b.is_empty()).map_err(|err| err.into())
    }
}

pub fn header<Q: QshRead>(parser: &mut Q) -> Result<Header, QshError> {
    // [..19] == qscalp signature
    let signature: &[u8] = &[
        0x51, 0x53, 0x63, 0x61, 0x6c, 0x70, 0x20, 0x48, 0x69, 0x73, 0x74, 0x6f, 0x72, 0x79, 0x20,
        0x44, 0x61, 0x74, 0x61,
    ];
    if !parser.consume_with(signature.len(), |buf| buf.eq(&signature[..]))? {
        return Err(QshError::Validation(
"Проверка сигнатуры QScalp файла трагически провалилась. Скорее всего это не *.qsh файл.".into()
                        ));
    }

    // version == 4
    let version = parser.byte().and_then(|version| {
        if version != 4 {
            Err(QshError::Validation(format!(
                "Неподдерживаемая версия формата - {}, ожидается версия 4",
                version
            )))
        } else {
            Ok(version)
        }
    })?;

    let (recorder, comment, recording_time, stream_count) =
        (parser.string()?, parser.string()?, parser.i64()?, parser.byte()?);

    // stream count == 1
    match stream_count {
        0 => {
            return Err(QshError::Validation(
                "файл не содержит потоков данных, вообще, ни единого".into(),
            ))
        }
        x if x > 1 => {
            return Err(QshError::Validation(format!(
                "stream_count={}. Мульти-стрим qsh-файлы не поддерживаются, за ненадобностью.",
                stream_count
            )))
        }
        _ => (),
    };

    let recording_time = i64::max(recording_time, 0);
    let (stream_type, instrument) = (parser.byte()?, parser.string()?);
    Ok(Header {
        version,
        recorder,
        comment,
        recording_time,
        stream: stream_type.into(),
        instrument,
    })
}

pub struct RecordIter<T, Q>(T, Q);

impl<T: QshParser, Q: QshRead> Iterator for RecordIter<T, Q> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1.eof().unwrap() {
            None
        } else {
            Some(self.0.parse(&mut self.1).unwrap())
        }
    }
}
