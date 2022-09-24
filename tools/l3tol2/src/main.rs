use ah::Context;
use anyhow as ah;
use clap::Parser;
use faccess::PathExt;
use flate2::bufread::GzDecoder;
use qsh_rs::{types::Stream, QshRead};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};

/// Reads standard input for the paths to the qsh files containing L3 market data, and produces L2 incremental events for each file.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Depth of the produced l2 book, '0' for unlimited
    #[clap(short, long, value_parser, default_value_t = 0)]
    depth: u16,

    /// Path to save files in if specified, otherwise outputs to stdout
    #[clap(parse(from_os_str))]
    output: Option<PathBuf>,
}

fn main() -> ah::Result<()> {
    let args = Args::parse();

    // collect input, validate
    let mut inputs = Vec::with_capacity(50);
    for line in std::io::stdin().lock().lines() {
        // file
        let line = line?;

        // do exists
        let path = std::fs::canonicalize(PathBuf::from(line.clone()))
            .with_context(|| format!("input file \"{line}\" not exists"))?;

        // is readable
        if !path.readable() {
            ah::bail!("{path:?} is not readable");
        }

        // is '*.qsh'
        if path.extension().and_then(|ext| ext.to_str()).filter(|ext| ext == &"qsh").is_none() {
            ah::bail!("{path:?} is not a 'QScalp History File(qsh)'");
        }

        // is valid qsh file of expected stream type
        let mut buf = [0u8; 1 << 10];
        File::open(path.clone()).map(BufReader::new).map(GzDecoder::new).and_then(|mut f| {
            let res = f.read_exact(&mut buf[..]);
            drop(f);
            res
        })?;
        let header = qsh_rs::header(&mut QshParser::new(buf.to_vec()))
            .with_context(|| format!("failed to read qsh header from {path:?}"))?;

        if header.stream != Stream::ORDERLOG {
            ah::bail!(
                "failed to validate {path:?}\n{header:?}\n expecting file of 'Stream::ORDERLOG' stream type"
            );
        }

        inputs.push(path);
    }

    // validate output path
    let output = if let Some(path) = &args.output {
        let output = std::fs::canonicalize(path)
            .with_context(|| format!("output path {path:?} not exists/reachable"))?;
        if !output.is_dir() {
            ah::bail!("output path {output:?} is not a directory");
        } else if !output.writable() {
            ah::bail!("output path {output:?} is not writable");
        }
        Some(output)
    } else {
        None
    };

    // process
    let stats = l3tol2::schedule(inputs, output, args.depth as usize);
    //println!("{stats:?}");

    Ok(())
}

// 05.09.14-01.05.21 ~ 7 years
