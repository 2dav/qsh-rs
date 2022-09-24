use crate::{
    orderbook::{self as ob, L2Event, PartitionBy},
    types::{Event, OLFlags, OrderLog},
};

struct L3L2Converter<I> {
    inner: I,
    book: ob::OrderBook,
    depth: usize,
}

impl<I> L3L2Converter<I> {
    fn new(inner: I, depth: usize) -> Self {
        Self { inner, book: Default::default(), depth }
    }

    fn process(&mut self, tx: Vec<OrderLog>) -> Vec<L2Event> {
        let mut events = Vec::with_capacity(100);
        for rec in tx {
            match Event::from(&rec) {
                Event::Add => self.book.add(rec, &mut events),
                Event::Fill => self.book.trade(rec, &mut events),
                Event::Cancel | Event::Remove => self.book.cancel(rec, &mut events),
                Event::UNKNOWN => unreachable!(),
            };
        }
        events
    }
}

impl<Inner> Iterator for L3L2Converter<Inner>
where
    Inner: Iterator<Item = Vec<OrderLog>>,
{
    type Item = Vec<L2Event>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|tx|{
            if OLFlags::NewSession % tx[0].order_flags {
                self.book.clear();
                assert_eq!(tx.len(), 1,
                           "this means new session message is not coming independent and following messages should be handled as well");
                vec![L2Event::Clear]
            } else {
                self.process(tx)
            }
        })
    }
}

pub fn convert(
    input: impl Iterator<Item = OrderLog>,
    depth: usize,
) -> impl Iterator<Item = L2Event> {
    L3L2Converter::new(
        input.filter(ob::system_record).partition_by(ob::tx_end).filter(ob::fiok_with_trades),
        depth,
    )
    .flatten()
}
