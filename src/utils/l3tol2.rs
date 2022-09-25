/// MOEX L3 messages to L2 messages conversion routine
///
use crate::{
    orderbook::{self as ob, PartitionBy},
    types::{L2Message, L3Message, OLFlags, OrderLog},
};

use super::moex2conv::moex_to_l3;

struct L3L2Converter<I> {
    inner: I,
    book: ob::OrderBook,
    depth: usize,
}

impl<I> L3L2Converter<I> {
    fn new(inner: I, depth: usize) -> Self {
        Self { inner, book: Default::default(), depth }
    }

    fn process(&mut self, tx: Vec<OrderLog>) -> Vec<L2Message> {
        moex_to_l3(tx).fold(Vec::<L2Message>::new(), |mut events, a| {
            match a {
                L3Message::Add(rec) => self.book.add(rec, &mut events),
                L3Message::Cancel(rec) => self.book.cancel(rec, &mut events),
                L3Message::Trade(rec) => self.book.trade(rec, &mut events),
            };
            events
        })
    }
}

impl<Inner> Iterator for L3L2Converter<Inner>
where
    Inner: Iterator<Item = Vec<OrderLog>>,
{
    type Item = Vec<L2Message>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|tx| {
            if OLFlags::NewSession % tx[0].order_flags {
                self.book.clear();
                vec![L2Message::Clear]
            } else {
                self.process(tx)
            }
        })
    }
}

pub fn convert(
    input: impl Iterator<Item = OrderLog>,
    depth: usize,
) -> impl Iterator<Item = L2Message> {
    L3L2Converter::new(
        input.filter(ob::system_record).partition_by(ob::tx_end).filter(ob::fiok_with_trades),
        depth,
    )
    .flatten()
}
