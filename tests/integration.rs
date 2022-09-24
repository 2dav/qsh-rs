use qsh_rs::{
    deflate, header, AuxInfoReader, DealReader, OrderLogReader, QshParser, QshRead, QuotesReader,
};

fn parse<T: QshParser>(f: &str) {
    let mut parser = deflate(f.into()).unwrap();
    print!("{}\n{:#?}\n", f, header(&mut parser).unwrap());
    let iter = parser.into_iter::<T>();
    println!("{}", iter.count());
}

#[test]
fn orderlog() {
    parse::<OrderLogReader>("data/zerich/Si-3.20.2020-03-17.OrdLog.qsh");
}

#[test]
fn quotes() {
    parse::<QuotesReader>("data/zerich/USD000UTSTOM.2020-03-17.Quotes.qsh");
    parse::<QuotesReader>("data/erinrv/Si-3.20_FT.2020-03-17.Quotes.qsh");
    parse::<QuotesReader>("data/erinrv/USD000UTSTOM.2020-03-17.Quotes.qsh");
}

#[test]
fn deals() {
    parse::<DealReader>("data/zerich/SBER.2020-03-17.Deals.qsh");
    parse::<DealReader>("data/erinrv/SBER.2020-03-17.Deals.qsh");
}

#[test]
fn aux() {
    parse::<AuxInfoReader>("data/zerich/SBER.2020-03-17.AuxInfo.qsh");
    parse::<AuxInfoReader>("data/erinrv/SBER.2020-03-17.AuxInfo.qsh");
}
