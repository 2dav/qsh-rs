use qsh_rs::orderbook::{self as ob, PartitionBy};
use qsh_rs::types::{Event, OLFlags, Side};
use qsh_rs::{deflate, header, OrderLogReader, QshRead};

fn main() {
    let mut parser = deflate("data/zerich/Si-3.20.2020-03-17.OrdLog.qsh".into()).unwrap();
    header(&mut parser).unwrap();

    let mut book: ob::OrderBook = Default::default();

    let iter = parser
        .into_iter::<OrderLogReader>()
        .filter(ob::system_record)
        .partition_by(ob::tx_end)
        .filter(ob::fiok_with_trades);

    for tx in iter {
        if OLFlags::NewSession % tx[0].order_flags {
            book.clear();
        }
        tx.into_iter().for_each(|r| match Event::from(&r) {
            Event::Add => book.add(r, None),
            Event::Fill => book.trade(r, None),
            Event::Cancel | Event::Remove => book.cancel(r, None),
            Event::UNKNOWN => unreachable!(),
        });

        if book.depth(Side::Buy) >= 5 && book.depth(Side::Sell) >= 5 {
            println!("{:?}", book.snapshot(5));
            println!("{}", book.mid_price());
        }
    }
}
