use flate2::bufread::GzDecoder;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};
pub mod orderbook;
mod read;
pub mod types;
pub use read::{AuxInfoReader, DealReader, OrderLogReader, QshReader, QuotesReader};

use crate::types::Header;
use anyhow as ah;
use leb128::read as leb128;
use std::io::Cursor;

pub fn deflate(path: PathBuf) -> Result<Vec<u8>, std::io::Error> {
    let mut buf = Vec::with_capacity(50 << 20);
    File::open(path)
        .map(BufReader::new)
        .map(GzDecoder::new)?
        .read_to_end(&mut buf)?;
    Ok(buf)
}

pub struct QshParser {
    inner: Vec<u8>,
    position: usize,
}

impl QshParser {
    pub fn new(bytes: Vec<u8>) -> Self {
        QshParser {
            inner: bytes,
            position: 0,
        }
    }

    #[inline(always)]
    pub fn eof(&self) -> bool {
        self.position == self.inner.len()
    }

    #[inline(always)]
    pub(crate) fn consume(&mut self, n: usize) -> ah::Result<&[u8]> {
        let pos = self.position;
        if pos + n > self.inner.len() {
            ah::bail!(
                "requested size({}) is out of range({}) at position {}",
                n,
                self.inner.len(),
                self.position
            );
        }
        self.position += n;
        Ok(&self.inner[pos..pos + n])
    }

    pub fn byte(&mut self) -> ah::Result<u8> {
        self.consume(1).map(|s| s[0])
    }

    pub fn i64(&mut self) -> ah::Result<i64> {
        self.u64().map(|u| u as i64)
    }

    pub fn u64(&mut self) -> ah::Result<u64> {
        let mut res = 0u64;
        for (index, &byte) in self.consume(8)?.iter().enumerate() {
            res += (byte as u64) << (8 * index);
        }
        Ok(res)
    }

    pub fn f64(&mut self) -> ah::Result<f64> {
        self.u64().map(f64::from_bits)
    }

    pub fn i16(&mut self) -> ah::Result<i16> {
        self.u16().map(|u| u as i16)
    }

    pub fn u16(&mut self) -> ah::Result<u16> {
        let mut res = 0u16;
        for (index, &byte) in self.consume(2)?.iter().enumerate() {
            res += (byte as u16) << (8 * index);
        }
        Ok(res)
    }

    pub fn uleb(&mut self) -> ah::Result<u64> {
        let mut cursor = Cursor::new(&self.inner[self.position..]);
        let res = leb128::unsigned(&mut cursor)?;
        //advance inner position to the number of bytes read
        let new_pos = cursor.position() as usize;
        self.consume(new_pos).unwrap();
        Ok(res)
    }

    pub fn leb(&mut self) -> ah::Result<i64> {
        let mut cursor = Cursor::new(&self.inner[self.position..]);
        let res = leb128::signed(&mut cursor)?;
        //advance inner position to the number of bytes read
        let new_pos = cursor.position() as usize;
        self.consume(new_pos).unwrap();
        Ok(res)
    }

    pub fn growing(&mut self) -> ah::Result<i64> {
        match self.uleb()? {
            268_435_455 => self.leb(),
            x => Ok(x as i64),
        }
    }

    pub fn string(&mut self) -> ah::Result<String> {
        self.leb()
            .and_then(|n| self.consume(n as usize))
            .and_then(|b| std::str::from_utf8(b).map_err(|e| ah::anyhow!(e)))
            .map(|s| s.to_owned())
    }

    pub fn into_iter<T: QshReader>(self) -> impl Iterator<Item = T::Item> {
        RecordIter(T::default(), self)
    }
}

pub fn header(parser: &mut QshParser) -> ah::Result<Header> {
    let qscalp_signature: &[u8] = &[
        0x51, 0x53, 0x63, 0x61, 0x6c, 0x70, 0x20, 0x48, 0x69, 0x73, 0x74, 0x6f, 0x72, 0x79, 0x20,
        0x44, 0x61, 0x74, 0x61,
    ];
    let sig = parser.consume(qscalp_signature.len())?;
    assert_eq!(
        sig, qscalp_signature,
        "Проверка сигнатуры QScalp файла трагически провалилась. Скорее всего это не *.qsh файл."
    );
    let version = parser.byte()?;
    assert_eq!(version, 4, "Неподдерживаемая версия формата - {}", version);
    let (recorder, comment, recording_time, stream_count) = (
        parser.string()?,
        parser.string()?,
        parser.i64()?,
        parser.byte()?,
    );
    assert_eq!(
        stream_count, 1,
        "stream_count={}. Мульти-стрим qsh-файлы не поддерживаются за ненадобностью. Создайте issue на github если вам это нужно.",
        stream_count
    );
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

pub struct RecordIter<T>(T, QshParser);

impl<T: QshReader> Iterator for RecordIter<T> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1.eof() {
            None
        } else {
            Some(self.0.parse(&mut self.1).unwrap())
        }
    }
}
