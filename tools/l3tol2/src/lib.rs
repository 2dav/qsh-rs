use anyhow::{self as ah, Context};
use bincode::{config, encode_into_std_write};
use flate2::{write::GzEncoder, Compression};
use qsh_rs::{inflate, utils::l3tol2::convert, OrderLogReader, QshRead};
use rayon::prelude::*;
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
    _input: PathBuf,
    len: usize,
}

fn process_job(Job { input, output, depth }: Job) -> ah::Result<Stat> {
    let mut bytes = inflate(input.to_path_buf())?;
    let _ = qsh_rs::header(&mut bytes)?;
    let reader = bytes.into_iter::<OrderLogReader>();

    let mut encoder =
        GzEncoder::new(BufWriter::with_capacity(50 << 20, output), Compression::best());
    let config = config::standard();
    let mut stat = Stat { _input: input, len: 0 };
    for tx in convert(reader, depth) {
        let tx = tx?;
        stat.len += tx.len();
        for msg in tx {
            encode_into_std_write(msg, &mut encoder, config)?;
        }
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
