use crate::types::{L2Message, OLFlags, OrderLog, OrderType, Price, Side, Timestamp, Volume};

pub type MidPrice = f64;
pub type Snapshot = (Timestamp, Vec<i64>);
pub type Level = (Price, Volume, Vec<OrderLog>);
pub type Quote = (Price, Volume);

#[derive(Debug, Default)]
pub struct OrderBook(Vec<Level>, Vec<Level>, Timestamp);

// TODO: clean all this 'assert' mess in favor of error propagation

impl OrderBook {
    pub fn add<'a, I>(&'a mut self, rec: OrderLog, events: I)
    where
        I: Into<Option<&'a mut Vec<L2Message>>>,
    {
        assert_eq!(OLFlags::Fill % rec.order_flags, false, "is Fill");
        assert_eq!(OLFlags::Canceled % rec.order_flags, false, "is Canceled");
        assert_eq!(OLFlags::CanceledGroup % rec.order_flags, false, "is CanceledGroup");
        assert_ne!(rec.amount_rest, 0, "{}", ol_msg("amount_rest == 0", rec),);
        assert_eq!(rec.amount, rec.amount_rest);

        let size = match self.find_level(rec.side, rec.price) {
            (Err(ix), side) => {
                side.insert(ix, (rec.price, rec.amount, vec![rec]));
                rec.amount
            }
            (Ok(ix), side) => {
                let lvl = side.get_mut(ix).unwrap();
                lvl.2.push(rec);
                lvl.1 += rec.amount;
                lvl.1
            }
        };

        events.into().map(|e| e.push(L2Message::Quote { side: rec.side, price: rec.price, size }));

        self.2 = ticks_to_unix_time(rec.timestamp);
    }

    pub fn cancel<'a, I>(&mut self, rec: OrderLog, events: I)
    where
        I: Into<Option<&'a mut Vec<L2Message>>>,
    {
        assert!(false == OLFlags::Fill % rec.order_flags, "{}", ol_msg("is Fill", rec));
        assert!(false == OLFlags::Add % rec.order_flags, "is Add");

        if let (Ok(ix), side) = self.find_level(rec.side, rec.price) {
            let level = &mut side.get_mut(ix).unwrap();
            let tgt = level.2.iter().position(|r| r.order_id == rec.order_id);

            match (tgt, rec.amount_rest) {
                (Some(i), 0) => {
                    let diff = level.2[i].amount;
                    assert!(level.1 >= diff);
                    level.2.remove(i);
                    level.1 -= diff;
                    if level.2.len() == 0 {
                        assert_eq!(level.1, 0);
                        side.remove(ix);

                        events.into().map(|e| {
                            e.push(L2Message::Remove { side: rec.side, price: rec.price })
                        });
                    } else if level.1 == 0 {
                        panic!("there are some active orders left at the level, but total level volume is 0");
                    } else {
                        events.into().map(|e| {
                            e.push(L2Message::Quote {
                                side: rec.side,
                                price: rec.price,
                                size: level.1,
                            })
                        });
                    }
                }
                (Some(i), rest) => {
                    assert!(level.2[i].amount > rest);
                    let diff = level.2[i].amount - rest;
                    assert!(level.1 > diff);
                    level.1 -= diff;
                    level.2[i].amount = rest;
                    level.2[i].amount_rest = rest;

                    events.into().map(|e| {
                        e.push(L2Message::Quote { side: rec.side, price: rec.price, size: level.1 })
                    });
                }
                _ => unreachable!(
                    "order to remove not found in level {:#?},{}",
                    level,
                    ol_msg("", rec)
                ),
            };
        } else {
            panic!("level not found, {:#?}", rec)
        }

        self.2 = ticks_to_unix_time(rec.timestamp);
    }

    pub fn trade<'a, I>(&mut self, rec: OrderLog, events: I)
    where
        I: Into<Option<&'a mut Vec<L2Message>>>,
    {
        assert_eq!(OLFlags::Add % rec.order_flags, false, "is Add");
        assert_eq!(OLFlags::Canceled % rec.order_flags, false, "is Canceled");
        assert_eq!(OLFlags::CanceledGroup % rec.order_flags, false, "is CanceledGroup");
        assert_ne!(rec.amount, 0);

        match self.find_level(rec.side, rec.price) {
            (Err(_), _) => panic!("level do not exists while modifying"),
            (Ok(ix), side) => {
                let level = &mut side.get_mut(ix).unwrap();
                if let Some(i) = level.2.iter().position(|r| r.order_id == rec.order_id) {
                    let order = level.2.get_mut(i).unwrap();
                    if order.amount == rec.amount {
                        level.2.remove(i);
                    } else {
                        assert!(order.amount > rec.amount, "{order:#?} {rec:#?}");
                        assert!(order.amount_rest > rec.amount);
                        order.amount -= rec.amount;
                        order.amount_rest -= rec.amount;
                    }
                    assert!(level.1 >= rec.amount);
                    level.1 -= rec.amount;
                } else {
                    panic!("order to modify not found in level {}", ol_msg("", rec));
                }

                if level.2.len() == 0 {
                    assert_eq!(level.1, 0);
                    side.remove(ix);
                    events
                        .into()
                        .map(|e| e.push(L2Message::Remove { side: rec.side, price: rec.price }));
                } else if level.1 == 0 {
                    println!("{:#?}, {:#?}", level.2, rec);
                    panic!("level volume is 0, but there are some active orders left");
                } else {
                    events.into().map(|e| {
                        e.push(L2Message::Quote { side: rec.side, price: rec.price, size: level.1 })
                    });
                }
            }
        }

        self.2 = ticks_to_unix_time(rec.timestamp);
    }
}

impl OrderBook {
    #[inline(always)]
    fn find_level(&mut self, side: Side, price: Price) -> (Result<usize, usize>, &mut Vec<Level>) {
        match side {
            Side::Buy => (self.0.binary_search_by(|(p, _, _)| price.cmp(&p)), &mut self.0),
            Side::Sell => (self.1.binary_search_by(|(p, _, _)| p.cmp(&price)), &mut self.1),
            Side::UNKNOWN => unreachable!(),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
        self.1.clear();
    }

    #[inline]
    pub fn depth(&self, side: Side) -> usize {
        if side == Side::Buy {
            self.0.len()
        } else {
            self.1.len()
        }
    }

    #[inline]
    pub fn level_summary(&self, side: Side, depth: usize) -> (Price, Volume) {
        let (p, v, _) = if side == Side::Buy { &self.0[depth] } else { &self.1[depth] };
        (*p, *v)
    }

    pub fn snapshot(&self, depth: usize) -> Snapshot {
        (
            self.2,
            (0..depth).into_iter().fold(vec![0; depth * 4], |mut snapshot, i| {
                let j = i * 4;
                assert!(self.0[i].1 > 0);
                assert!(self.1[i].1 > 0);
                snapshot[j + 0] = self.0[i].0;
                snapshot[j + 1] = self.0[i].1;
                snapshot[j + 2] = self.1[i].0;
                snapshot[j + 3] = self.1[i].1;
                snapshot
            }),
        )
    }

    #[inline]
    pub fn mid_price(&self) -> MidPrice {
        (self.0[0].0 + self.1[0].0) as MidPrice * 0.5
    }
}

fn ol_msg(msg: &str, rec: OrderLog) -> String {
    format!("{}\n{rec}", msg,)
}

#[inline(always)]
pub fn fiok_with_trades(tx: &Vec<OrderLog>) -> bool {
    match OrderType::from(tx[0].order_flags) {
        OrderType::IOK | OrderType::FOK => tx.len() > 2,
        _ => true,
    }
}

#[inline(always)]
pub fn non_system_record(rec: &OrderLog) -> bool {
    OLFlags::NonSystem % rec.order_flags
        || OLFlags::NonZeroReplAct % rec.order_flags
        || rec.side == Side::UNKNOWN
}

#[inline(always)]
pub fn system_record(rec: &OrderLog) -> bool {
    !non_system_record(rec)
}

#[inline(always)]
pub fn tx_end(rec: &OrderLog) -> bool {
    OLFlags::TxEnd % rec.order_flags
}

/// windows 100ns ticks to unix time
#[inline]
pub fn ticks_to_unix_time(v: Timestamp) -> Timestamp {
    v - 62135596800000
}

pub struct Partition<I, KeyFn>
where
    I: Iterator,
{
    iter: I,
    split_fn: KeyFn,
    acc: Vec<I::Item>,
}

pub trait PartitionBy: Iterator {
    fn partition_by<F>(self, f: F) -> Partition<Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> bool,
    {
        Partition { iter: self, split_fn: f, acc: vec![] }
    }
}

impl<I> PartitionBy for I where I: Iterator {}

impl<I, F, T> Iterator for Partition<I, F>
where
    I: Iterator<Item = T>,
    T: Clone,
    F: FnMut(&I::Item) -> bool,
{
    type Item = Vec<I::Item>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(x) = self.iter.next() {
            let split = (self.split_fn)(&x);
            self.acc.push(x);
            if split {
                return Some(std::mem::replace(&mut self.acc, Vec::with_capacity(10)));
            }
        }
        None
    }
}
