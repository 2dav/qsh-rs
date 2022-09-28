/// MOEX architecture-specific orderlog(L3) messages to a common format conversion routin  
///
use crate::{
    types::{L3Message, OLMsgType, OrderLog, OrderType},
    QshError,
};

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
                (OLMsgType::Remove, _) => {
                    assert_eq!(ord_t, OrderType::IOK, "unreachable, logic error")
                }
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

pub fn moex_to_l3(tx: Vec<OrderLog>) -> impl Iterator<Item = Result<Vec<L3Message>, QshError>> {
    chunks(tx).into_iter().map(move |c| match c {
        Chunk::Order(rec) => match OLMsgType::from(&rec) {
            OLMsgType::Add => Ok(vec![L3Message::Add(rec)]),
            OLMsgType::Cancel => Ok(vec![L3Message::Cancel(rec)]),
            _ => unreachable!(),
        },
        Chunk::Trades(src, tgt) if src.len() == 1 => {
            // [[o], [x*]]
            // one added order that cause one-or-many trades
            let mut src = src[0];
            let src_t = OrderType::from(src.order_flags);

            let mut acts = tgt
                .into_iter()
                .flat_map(|rec| {
                    if rec.order_id == src.order_id {
                        if src.amount < rec.amount {
                            return Some(Err(QshError::InvalidState(
                                "rec.amount > src.amount".to_string(),
                            )));
                        }
                        if src.amount_rest < rec.amount {
                            return Some(Err(QshError::InvalidState(
                                "rec.amount > src.amount_rest".to_string(),
                            )));
                        }
                        src.amount -= rec.amount;
                        src.amount_rest -= rec.amount;
                        None
                    } else {
                        Some(Ok(L3Message::Trade(rec)))
                    }
                })
                .collect::<Result<Vec<L3Message>, QshError>>()?;

            if src.amount_rest > 0 && src_t == OrderType::Limit {
                acts.push(L3Message::Add(src));
            }

            Ok(acts)
        }
        Chunk::Trades(mut src, tgt) => {
            // [[o*], [x*]]
            // special case of matching between orders added within the same transaction.
            // NOTE: This implementation ignores such orders if they don't hit the book.
            // [[+1], [-1], [+2]] -> [[+2]]

            src.sort_by_key(|rec| rec.order_id);
            let mut actions = tgt
                .into_iter()
                .filter_map(|rec| {
                    if OLMsgType::from(&rec) != OLMsgType::Fill {
                        return Some(Err(QshError::Validation("wrong orderlog type".to_string())));
                    }
                    match src.binary_search_by_key(&rec.order_id, |&rec| rec.order_id) {
                        Ok(ix) => {
                            let src = unsafe { src.get_unchecked_mut(ix) };
                            if src.amount < rec.amount {
                                return Some(Err(QshError::InvalidState(
                                    "src.amount < rec.amount".to_string(),
                                )));
                            }
                            if src.amount_rest < rec.amount {
                                return Some(Err(QshError::InvalidState(
                                    "src.amount_rest < rec.amount".to_string(),
                                )));
                            }
                            src.amount -= rec.amount;
                            src.amount_rest -= rec.amount;
                            None
                        }
                        Err(_) => Some(Ok(L3Message::Trade(rec))),
                    }
                })
                .collect::<Result<Vec<L3Message>, QshError>>()?;

            src.into_iter()
                .filter(|rec| {
                    rec.amount_rest > 0 && OrderType::from(rec.order_flags) == OrderType::Limit
                })
                .for_each(|rec| {
                    let mut rec = rec.clone();
                    rec.amount = rec.amount_rest;
                    actions.push(L3Message::Add(rec));
                });

            Ok(actions)
        }
    })
}
