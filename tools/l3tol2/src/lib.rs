use anyhow::{self as ah, Context};
use bincode::{config, encode_into_std_write};
use flate2::{write::GzEncoder, Compression};
use qsh_rs::{deflate, types::OrderLog, utils::l3tol2::convert, OrderLogReader, QshParser};
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
};

struct Job {
    input: PathBuf,
    output: Box<dyn Write>,
    depth: usize,
}

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

#[derive(Debug)]
pub struct Stat {
    input: PathBuf,
    len: usize,
}

fn ordlog_iter(bytes: Vec<u8>) -> ah::Result<impl Iterator<Item = OrderLog>> {
    let mut parser = QshParser::new(bytes);
    qsh_rs::header(&mut parser)?;
    Ok(parser.into_iter::<OrderLogReader>())
}

fn process_job(Job { input, output, depth }: Job) -> ah::Result<Stat> {
    let ordlogs =
        deflate(input.to_path_buf()).map_err(|err| ah::anyhow!(err)).and_then(ordlog_iter)?;
    let mut encoder =
        GzEncoder::new(BufWriter::with_capacity(50 << 20, output), Compression::best());
    let config = config::standard();
    let mut stat = Stat { input, len: 0 };
    for (i, rec) in convert(ordlogs, depth).enumerate() {
        encode_into_std_write(rec, &mut encoder, config)?;
        stat.len = i;
    }
    encoder.finish()?.flush()?;

    Ok(stat)
}

fn out_sink(input: &PathBuf, output: Option<PathBuf>) -> ah::Result<Box<dyn Write>> {
    match output {
        Some(ref dir) => {
            let fname = input.file_name().unwrap().to_string_lossy();
            let file_path = dir.join(&fname[..fname.len() - 4]).with_extension("bin");
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(file_path.clone())
                .with_context(|| format!("failed to create output file {file_path:?}"))?;
            Ok(Box::new(file))
        }
        None => Ok(Box::new(std::io::stdout().lock())),
    }
}

pub fn schedule(
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    depth: usize,
) -> Vec<ah::Result<Stat>> {
    inputs
        .into_par_iter()
        .map(|input| {
            out_sink(&input, output.clone())
                .map(|out| Job { output: out, input, depth })
                .and_then(process_job)
        })
        .collect::<_>()
}

#[cfg(test)]
mod tests {
    use bincode;
    use flate2::bufread::GzDecoder;
    use qsh_rs::{deflate, utils::l3tol2::L2Record};
    use std::{
        fs::File,
        io::{BufReader, Read},
    };

    //#[test]
    fn read() {
        let file = "/home/zood/Documents/Projects/trading/utils/qsh/qsh-rs/tools/l3tol2/target/HYDR-3.20.2020-01-03.bin";
        let config = bincode::config::standard();
        let mut input = File::open(file).map(BufReader::new).map(GzDecoder::new).unwrap();
        //let bytes = deflate(file.into()).unwrap();
        let mut i = 0;
        while let Ok(record) = bincode::decode_from_std_read::<L2Record, _, _>(&mut input, config) {
            i += 1;
        }
        //let res: Vec<L2Record> = bincode::decode_from_slice(bytes.as_slice(), config).unwrap().0;
        //println!("{}", res.len());
        //println!("{res:?}");
        println!("{i}");
    }
}
