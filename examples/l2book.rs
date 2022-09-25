use qsh_rs::{header, inflate, QshRead, QuotesReader};

fn main() {
    let mut parser = inflate("data/zerich/USD000UTSTOM.2020-03-17.Quotes.qsh".into()).unwrap();
    let header = header(&mut parser).unwrap();
    print!("{:#?}\n", header);
    let iter = parser.into_iter::<QuotesReader>();
    for q in iter {
        println!("{:?}", q.frame_time_delta);
        println!("Bid: {:?} \nAsk: {:?}", q.bid, q.ask);
    }
}
