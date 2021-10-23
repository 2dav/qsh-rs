use ndarray::Array2;
use numpy::{IntoPyArray, PyArray2};
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

use qsh_rs::orderbook::{self as ob, PartitionBy};
use qsh_rs::types::{Event, OLFlags, Side};
use qsh_rs::{deflate, header, OrderLogReader, QshParser};

#[pyfunction]
pub fn lob(file: String, depth: usize) -> PyResult<Py<PyArray2<i64>>> {
    let bytes = deflate(file.into()).unwrap();
    let mut parser = QshParser::new(bytes);
    header(&mut parser).unwrap();

    let mut book: ob::OrderBook = Default::default();

    let snapshots = parser
        .into_iter::<OrderLogReader>()
        .filter(ob::system_record)
        .partition_by(ob::tx_end)
        .filter(ob::fiok_with_trades)
        .fold(Vec::with_capacity(10 << 20), |mut acc, tx| {
            if OLFlags::NewSession % tx[0].order_flags {
                book.clear();
            }
            tx.into_iter().for_each(|r| match Event::from(&r) {
                Event::Add => book.add(r),
                Event::Fill => book.trade(r),
                Event::Cancel | Event::Remove => book.cancel(r),
                Event::UNKNOWN => unreachable!(),
            });
            if book.depth(Side::Buy) >= depth && book.depth(Side::Sell) >= depth {
                let (ts, s) = book.snapshot(depth);
                acc.push(ts);
                acc.extend(s);
            }
            acc
        });

    let row_size = depth * 2 * 2 + 1;
    let output_shape = (snapshots.len() / row_size, row_size);

    Ok(Python::with_gil(|py| {
        Array2::from_shape_vec(output_shape, snapshots)
            .unwrap()
            .into_pyarray(py)
            .to_owned()
    }))
}

#[pymodule]
fn pyqsh(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(lob, m)?)?;
    Ok(())
}
