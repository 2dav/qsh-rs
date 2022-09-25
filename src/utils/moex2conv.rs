/// MOEX architecture-specific orderlog(L3) messages to a common format conversion routine  
///
use std::collections::BTreeMap;

use crate::types::{L3Message, OLMsgType, OrderLog, OrderType};

type Record = (OrderLog, OLMsgType, OrderType);

enum Chunk {
    Order(Record),
    Trades(Vec<Record>, Vec<Record>),
}

fn record(rec: OrderLog) -> Record {
    (rec, OLMsgType::from(&rec), OrderType::from(rec.order_flags))
}

// group orders within transaction constituting trade events.
//
// o - add order(limit, iok/fok, cancel)
// x - execution event(fill)
//
// [o, o, o, x, x, o] -> [[o], [o, o, x, x], [o]]
// [o] - orders which have no place in the ongoing fills
// [o,o,x,x] orders causes a trades and their corresponding fill events
fn chunks(tx: Vec<OrderLog>) -> Vec<Chunk> {
    // TODO: optimize to use indices inside tx vec instead of copies
    // get rid of asserts in favor of errors propagation

    let fill_ids: Vec<i64> = tx
        .iter()
        .filter(|&rec| OLMsgType::from(rec) == OLMsgType::Fill)
        .map(|rec| rec.order_id)
        .collect();

    if fill_ids.len() == 0 {
        tx.into_iter()
            .map(record)
            .filter(|re| {
                let is_remove = re.1 == OLMsgType::Remove;
                if is_remove {
                    assert!(
                        false,
                        "we never hit this since IOK/FOK without trades are already filtered out, right?"
                    );
                    assert_eq!(re.2, OrderType::IOK);
                }
                !is_remove
            })
            .filter(|re| re.2 != OrderType::IOK && re.2 != OrderType::FOK)
            .map(|re| Chunk::Order(re))
            .collect::<Vec<Chunk>>()
    } else {
        let mut chunks: Vec<Chunk> = vec![];
        let mut src: Vec<Record> = vec![];
        let mut tgt: Vec<Record> = vec![];
        let res = tx.into_iter().map(record).collect::<Vec<Record>>();

        for re in res {
            match (re.1, fill_ids.contains(&re.0.order_id)) {
                (OLMsgType::Add, true) => src.push(re),
                (OLMsgType::Fill, true) => tgt.push(re),
                (OLMsgType::Fill, false) => unreachable!("should already be captured"),
                (OLMsgType::Remove, _) => {
                    assert_eq!(re.2, OrderType::IOK)
                }
                _ => {
                    if src.len() + tgt.len() > 0 {
                        chunks.push(Chunk::Trades(src.clone(), tgt.clone()));
                        src.clear();
                        tgt.clear();
                    }
                    if re.2 == OrderType::Limit {
                        chunks.push(Chunk::Order(re));
                    }
                }
            }
        }

        if src.len() + tgt.len() > 0 {
            chunks.push(Chunk::Trades(src, tgt));
        }

        chunks
    }
}

pub fn moex_to_l3(tx: Vec<OrderLog>) -> impl Iterator<Item = L3Message> {
    // TODO: asserts => error propagation
    chunks(tx).into_iter().flat_map(|c| match c {
        Chunk::Order(re) => {
            assert_eq!(re.2, OrderType::Limit);

            match re.1 {
                OLMsgType::Add => vec![L3Message::Add(re.0)],
                OLMsgType::Cancel => vec![L3Message::Cancel(re.0)],
                _ => unreachable!(),
            }
        }
        Chunk::Trades(src, tgt) if src.len() == 1 => {
            // [[o], [x*]]
            // one added order that cause one-or-many trades
            let (mut src, _, src_type) = src[0];
            let src_id = src.order_id;
            let mut acts = tgt
                .iter()
                .inspect(|re| {
                    assert_eq!(re.1, OLMsgType::Fill);
                    if re.0.order_id == src.order_id {
                        assert!(src.amount >= re.0.amount);
                        assert!(src.amount_rest >= re.0.amount);
                        src.amount -= re.0.amount;
                        src.amount_rest -= re.0.amount;
                    }
                })
                .filter(|(rec, _, _)| rec.order_id != src_id)
                .map(|re| L3Message::Trade(re.0))
                .collect::<Vec<L3Message>>();
            if src.amount_rest > 0 && src_type == OrderType::Limit {
                acts.push(L3Message::Add(src));
            }
            acts
        }
        Chunk::Trades(src, tgt) => {
            // [[o*], [x*]]
            // special case of matching between orders added within the same transaction.
            // NOTE: This implementation ignores such orders if they don't hit the book.
            // [[+1], [-1], [+2]] -> [[+2]]

            // TODO: could just use a vec instead of btree
            let mut map =
                src.iter().fold(BTreeMap::<i64, OrderLog>::new(), |mut acc, (rec, e, _)| {
                    assert_eq!(e, &OLMsgType::Add);
                    acc.insert(rec.order_id, rec.clone());
                    acc
                });
            let mut actions =
                tgt.into_iter().fold(Vec::<L3Message>::new(), |mut acc, (rec, e, _)| {
                    assert_eq!(e, OLMsgType::Fill);
                    if map.contains_key(&rec.order_id) {
                        let _src = map.get_mut(&rec.order_id).unwrap();
                        assert!(_src.amount >= rec.amount);
                        assert!(_src.amount_rest >= rec.amount);
                        _src.amount -= rec.amount;
                        _src.amount_rest -= rec.amount;
                    } else {
                        acc.push(L3Message::Trade(rec));
                    }
                    acc
                });
            map.values()
                .filter(|rec| rec.amount_rest > 0)
                .filter(|rec| OrderType::from(rec.order_flags) == OrderType::Limit)
                .for_each(|rec| {
                    assert_eq!(OLMsgType::from(rec), OLMsgType::Add);
                    let mut rec = rec.clone();
                    rec.amount = rec.amount_rest;
                    actions.push(L3Message::Add(rec));
                });
            actions
        }
    })
}
