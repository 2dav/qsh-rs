use ndarray::Array2;
use numpy::{IntoPyArray, PyArray2};
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

use qsh_rs::orderbook::{self as ob, PartitionBy};
use qsh_rs::types::OrderLog;
use qsh_rs::types::OrderType;
use qsh_rs::types::Timestamp;
use qsh_rs::types::{OLFlags, OLMsgType, Side};
use qsh_rs::{header, inflate, OrderLogReader, QshRead, QuotesReader};

#[inline]
fn ol_transactions(file: String) -> impl Iterator<Item = Vec<OrderLog>> {
    let mut parser = inflate(file.into()).unwrap();
    header(&mut parser).unwrap();

    parser
        .into_iter::<OrderLogReader>()
        .filter(ob::system_record)
        .partition_by(ob::tx_end)
        .filter(ob::fiok_with_trades)
}

#[pyfunction]
pub fn orders(file: String) -> PyResult<Py<PyArray2<i64>>> {
    let records = ol_transactions(file).fold(Vec::with_capacity(10 << 20), |mut acc, tx| {
        // [timestamp, order_id, event, order, side, price, amount] : i64
        //
        // event:
        // 0 - Add
        // 1 - Cancel
        //
        // order:
        // 0 - Limit
        // 1 - IOK
        // 2 - FOK
        //
        // side:
        // 1 - Buy
        // 2 - Sell

        tx.into_iter().for_each(|r| match OLMsgType::from(&r) {
            OLMsgType::Add => {
                let kind = match OrderType::from(r.order_flags) {
                    OrderType::Limit => 0,
                    OrderType::IOK => 1,
                    OrderType::FOK => 2,
                    _ => unreachable!("unknown order type"),
                };
                acc.extend([r.timestamp, r.order_id, 0, kind, r.side as i64, r.price, r.amount]);
            }
            OLMsgType::Cancel | OLMsgType::Remove => {
                if OrderType::from(r.order_flags) == OrderType::Limit {
                    acc.extend([r.timestamp, r.order_id, 1, 0, 0, 0, 0])
                }
            }
            OLMsgType::Fill => (),
            OLMsgType::UNKNOWN => unreachable!(),
        });

        acc
    });

    let row_size = 7;
    let output_shape = (records.len() / row_size, row_size);

    Ok(Python::with_gil(|py| {
        Array2::from_shape_vec(output_shape, records).unwrap().into_pyarray(py).to_owned()
    }))
}

#[pyfunction]
pub fn lob(file: String, depth: usize) -> PyResult<Py<PyArray2<i64>>> {
    let mut book: ob::OrderBook = Default::default();

    let snapshots = ol_transactions(file).fold(Vec::with_capacity(10 << 20), |mut acc, tx| {
        if OLFlags::NewSession % tx[0].order_flags {
            book.clear();
        }
        tx.into_iter().for_each(|r| {
            match OLMsgType::from(&r) {
                OLMsgType::Add => book.add(r, None),
                OLMsgType::Fill => book.trade(r, None),
                OLMsgType::Cancel | OLMsgType::Remove => book.cancel(r, None),
                OLMsgType::UNKNOWN => unreachable!(),
            }
            .unwrap()
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
        Array2::from_shape_vec(output_shape, snapshots).unwrap().into_pyarray(py).to_owned()
    }))
}

#[pyfunction]
pub fn quotes(file: String, depth: usize) -> PyResult<Py<PyArray2<i64>>> {
    let mut parser = inflate(file.into()).unwrap();
    let header = header(&mut parser).unwrap();
    let iter = parser.into_iter::<QuotesReader>();
    let unix_time_start = header.recording_time / 1e4 as Timestamp - 62135596800000;
    let quotes = iter
        .filter(|q| q.ask.len() >= depth && q.bid.len() >= depth)
        .fold((Vec::with_capacity(10 << 20), unix_time_start), |(mut vec, mut time), q| {
            time += q.frame_time_delta;
            vec.push(time);
            vec.extend(
                q.bid
                    .into_iter()
                    .take(depth)
                    .zip(q.ask.into_iter().take(depth))
                    .flat_map(|(b, a)| [b.0, b.1, a.0, a.1]),
            );
            (vec, time)
        })
        .0;
    let row_size = depth * 2 * 2 + 1;
    let output_shape = (quotes.len() / row_size, row_size);

    Ok(Python::with_gil(|py| {
        Array2::from_shape_vec(output_shape, quotes).unwrap().into_pyarray(py).to_owned()
    }))
}

#[pymodule]
fn pyqsh(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(lob, m)?)?;
    m.add_function(wrap_pyfunction!(orders, m)?)?;
    m.add_function(wrap_pyfunction!(quotes, m)?)?;
    Ok(())
}
