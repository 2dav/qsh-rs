use qsh_rs::orderbook::{self as ob, PartitionBy};
use qsh_rs::types::{Event, OLFlags, Side};
use qsh_rs::{
    deflate, header, AuxInfoReader, DealReader, OrderLogReader, QshParser, QshReader, QuotesReader,
};

fn test<T: QshReader>(f: &str) {
    let bytes = deflate(f.into()).unwrap();
    let mut parser = QshParser::new(bytes);
    print!("{}\n{:#?}\n", f, header(&mut parser).unwrap());
    let iter = parser.into_iter::<T>();
    println!("{}", iter.fold(0, |acc, _| acc + 1));
}

pub fn main() {
    test::<OrderLogReader>("data/zerich/Si-3.20.2020-03-17.OrdLog.qsh");
    test::<QuotesReader>("data/zerich/USD000UTSTOM.2020-03-17.Quotes.qsh");
    test::<DealReader>("data/zerich/SBER.2020-03-17.Deals.qsh");
    test::<AuxInfoReader>("data/zerich/SBER.2020-03-17.AuxInfo.qsh");

    test::<QuotesReader>("data/erinrv/Si-3.20_FT.2020-03-17.Quotes.qsh");
    test::<QuotesReader>("data/erinrv/USD000UTSTOM.2020-03-17.Quotes.qsh");
    test::<DealReader>("data/erinrv/SBER.2020-03-17.Deals.qsh");
    test::<AuxInfoReader>("data/erinrv/SBER.2020-03-17.AuxInfo.qsh");
    ob_example();
}

pub fn ob_example() {
    let bytes = deflate("data/zerich/Si-3.20.2020-03-17.OrdLog.qsh".into()).unwrap();
    let mut parser = QshParser::new(bytes);
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
            Event::Add => book.add(r),
            Event::Fill => book.trade(r),
            Event::Cancel | Event::Remove => book.cancel(r),
            Event::UNKNOWN => unreachable!(),
        });

        if book.depth(Side::Buy) >= 5 && book.depth(Side::Sell) >= 5 {
            println!("{:?}", book.snapshot(5));
            println!("{}", book.mid_price());
        }
    }
}
