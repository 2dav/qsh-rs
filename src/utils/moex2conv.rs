use std::collections::BTreeMap;

/// MOEX architecture-specific orderlog(L3) messages to a common format conversion routin  
///
use crate::types::{L3Message, OLMsgType, OrderLog, OrderType};

enum Chunk {
    Order(OrderLog),
    Trades(Vec<OrderLog>, Vec<OrderLog>),
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
    // TODO: get rid of asserts in favor of errors propagation

    let fill_ids: Vec<i64> = tx
        .iter()
        .filter_map(|rec| (OLMsgType::from(rec) == OLMsgType::Fill).then(|| rec.order_id))
        .collect();

    if fill_ids.len() == 0 {
        tx.into_iter()
            .filter_map(|rec| {
                let ord_t = OrderType::from(rec.order_flags);
                let is_remove = OLMsgType::from(&rec) == OLMsgType::Remove;
                let yield_ = !(is_remove || ord_t == OrderType::IOK || ord_t == OrderType::FOK);

                yield_.then(|| Chunk::Order(rec))
            })
            .collect::<Vec<Chunk>>()
    } else {
        let mut chunks: Vec<Chunk> = vec![];
        let mut src: Vec<OrderLog> = vec![];
        let mut tgt: Vec<OrderLog> = vec![];

        for rec in tx.into_iter() {
            let msg_t = OLMsgType::from(&rec);
            let in_fills = fill_ids.contains(&rec.order_id);
            let ord_t = OrderType::from(rec.order_flags);

            match (msg_t, in_fills) {
                (OLMsgType::Add, true) => src.push(rec),
                (OLMsgType::Fill, true) => tgt.push(rec),
                (OLMsgType::Fill, false) => unreachable!("should already be captured"),
                (OLMsgType::Remove, _) => assert_eq!(ord_t, OrderType::IOK),
                _ => {
                    if src.len() + tgt.len() > 0 {
                        chunks.push(Chunk::Trades(src.clone(), tgt.clone()));
                        src.clear();
                        tgt.clear();
                    }
                    if ord_t == OrderType::Limit {
                        chunks.push(Chunk::Order(rec));
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

pub fn moex_to_l3(mut tx: Vec<OrderLog>) -> impl Iterator<Item = L3Message> {
    // TODO: asserts => error propagation
    chunks(tx).into_iter().flat_map(move |c| match c {
        Chunk::Order(rec) => {
            assert_eq!(OrderType::from(rec.order_flags), OrderType::Limit);

            match OLMsgType::from(&rec) {
                OLMsgType::Add => vec![L3Message::Add(rec)],
                OLMsgType::Cancel => vec![L3Message::Cancel(rec)],
                _ => unreachable!(),
            }
        }
        Chunk::Trades(src, tgt) if src.len() == 1 => {
            // [[o], [x*]]
            // one added order that cause one-or-many trades
            let mut src = src[0];
            let src_t = OrderType::from(src.order_flags);
            let src_id = src.order_id;

            let mut acts = tgt
                .into_iter()
                .inspect(|rec| {
                    assert_eq!(OLMsgType::from(rec), OLMsgType::Fill);
                    if rec.order_id == src.order_id {
                        assert!(src.amount >= rec.amount);
                        assert!(src.amount_rest >= rec.amount);
                        src.amount -= rec.amount;
                        src.amount_rest -= rec.amount;
                    }
                })
                .filter_map(|rec| (rec.order_id != src_id).then(|| L3Message::Trade(rec)))
                .collect::<Vec<L3Message>>();

            if src.amount_rest > 0 && src_t == OrderType::Limit {
                acts.push(L3Message::Add(src));
            }

            acts
        }
        Chunk::Trades(mut src, tgt) => {
            // [[o*], [x*]]
            // special case of matching between orders added within the same transaction.
            // NOTE: This implementation ignores such orders if they don't hit the book.
            // [[+1], [-1], [+2]] -> [[+2]]

            src.sort_by_key(|rec| rec.order_id);

            let mut actions = tgt.into_iter().fold(Vec::<L3Message>::new(), |mut acc, rec| {
                assert_eq!(OLMsgType::from(&rec), OLMsgType::Fill);

                match src.binary_search_by_key(&rec.order_id, |&rec| rec.order_id) {
                    Ok(ix) => {
                        let src = unsafe { src.get_unchecked_mut(ix) };
                        assert!(src.amount >= rec.amount);
                        assert!(src.amount_rest >= rec.amount);
                        src.amount -= rec.amount;
                        src.amount_rest -= rec.amount;
                    }
                    Err(_) => acc.push(L3Message::Trade(rec)),
                };
                acc
            });

            src.into_iter()
                .filter(|rec| {
                    rec.amount_rest > 0 && OrderType::from(rec.order_flags) == OrderType::Limit
                })
                .for_each(|rec| {
                    assert_eq!(OLMsgType::from(&rec), OLMsgType::Add);
                    let mut rec = rec.clone();
                    rec.amount = rec.amount_rest;
                    actions.push(L3Message::Add(rec));
                });

            actions
        }
    })
}
