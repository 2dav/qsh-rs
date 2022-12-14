use crate::{
    types::{
        AuxInfo, AuxInfoFlags, Deal, DealFlags, OLEntryFlags, OLFlags, OLMsgType, OrderLog,
        OrderType, Price, Quotes, Side, Volume, UID,
    },
    QshError, QshRead,
};
use std::collections::BTreeMap;

pub trait QshParser: Default {
    type Item;
    fn parse(&mut self, parser: &mut impl QshRead) -> Result<Self::Item, QshError>;
}

// batch flag check - execute body block if bit flag is set
macro_rules! bitcheck {
    ($mask:ident { $($flag:expr => $body:expr),+}) => {
    $(if $flag % $mask {
        $body;
    })*
    };
}

// 'checked add' wrapper. panics on overflow.
macro_rules! cadd {
    ($tgt:expr, $value:expr) => {
        $tgt.checked_add($value).unwrap()
    };
}

// - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - OrderLog
#[derive(Default, Debug)]
pub struct OrderLogReader {
    prev: OrderLog,
    order_id: UID,
    deal_id: UID,
    deal_price: Price,
    oi: Volume,
}

impl QshParser for OrderLogReader {
    type Item = OrderLog;

    fn parse(&mut self, p: &mut impl QshRead) -> Result<Self::Item, QshError> {
        let (frame_time_delta, entry_flags, order_flags) = (p.growing()?, p.byte()?, p.u16()?);

        self.prev.frame_time_delta = frame_time_delta;
        self.prev.order_flags = order_flags;
        self.prev.entry_flags = entry_flags;

        bitcheck!(entry_flags {
            OLEntryFlags::DateTime => self.prev.timestamp = cadd!(self.prev.timestamp, p.growing()?),
            OLEntryFlags::OrderId  => if OLFlags::Add % order_flags{
                                          self.order_id = cadd!(self.order_id, p.growing()?);
                                          self.prev.order_id = self.order_id;
                                      } else{
                                          self.prev.order_id = cadd!(self.order_id, p.leb()?);
                                      },
            OLEntryFlags::Price    => self.prev.price = cadd!(self.prev.price, p.leb()?),
            OLEntryFlags::Amount   => self.prev.amount = p.leb()?
        });

        if !(OLEntryFlags::OrderId % entry_flags) {
            self.prev.order_id = self.order_id;
        }

        self.prev.amount_rest = 0;
        self.prev.deal_id = 0;
        self.prev.deal_price = 0;
        self.prev.oi = 0;

        bitcheck!(order_flags {
            OLFlags::Fill => {
                bitcheck!(entry_flags {
                    OLEntryFlags::AmountRest => self.prev.amount_rest = p.leb()?,
                    OLEntryFlags::DealId     => self.deal_id    = cadd!(self.deal_id, p.growing()?),
                    OLEntryFlags::DealPrice  => self.deal_price = cadd!(self.deal_price, p.leb()?),
                    OLEntryFlags::OI         => self.oi         = cadd!(self.oi, p.leb()?)
                });
                self.prev.deal_id    = self.deal_id;
                self.prev.deal_price = self.deal_price;
                self.prev.oi         = self.oi;
             },
            OLFlags::Add => self.prev.amount_rest = self.prev.amount
        });

        let buy = OLFlags::Buy % order_flags;
        let sell = OLFlags::Sell % order_flags;

        self.prev.side =
            match (buy, sell) {
                (true, true) => return Err(QshError::Parsing(
                    "ордер имеет одновременно установленные флаги 'bid' и 'ask' для стороны сделки"
                        .into(),
                )),
                (true, _) => Side::Buy,
                (_, true) => Side::Sell,
                _ => Side::UNKNOWN,
            };

        self.prev.type_ = OrderType::from(order_flags);
        self.prev.event = OLMsgType::from(&self.prev);

        Ok(self.prev.clone())
    }
}

// - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - Quotes
#[derive(Debug, Default)]
pub struct QuotesReader {
    map: BTreeMap<Price, Volume>,
    key: Price,
    q: Quotes,
}

impl QshParser for QuotesReader {
    type Item = Quotes;

    fn parse(&mut self, p: &mut impl QshRead) -> Result<Self::Item, QshError> {
        self.q.bid.clear();
        self.q.ask.clear();

        let (frame_time_delta, nrows) = (p.growing()?, p.leb()?);
        let mut quotes = self.q.clone();
        quotes.frame_time_delta = frame_time_delta;

        for _ in 0..nrows {
            self.key = cadd!(self.key, p.leb()?);
            let v = p.leb()?;
            if v == 0 {
                self.map.remove(&self.key).expect("key not found");
            } else {
                self.map.insert(self.key, v);
            }
        }

        self.map.iter().for_each(|(&k, &v)| {
            if v < 0 {
                quotes.bid.push((k, -v));
            } else {
                quotes.ask.push((k, v));
            }
        });

        Ok(quotes)
    }
}

// - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - Deals
#[derive(Debug, Default)]
pub struct DealReader {
    prev: Deal,
}

impl QshParser for DealReader {
    type Item = Deal;

    fn parse(&mut self, p: &mut impl QshRead) -> Result<Self::Item, QshError> {
        let (frame_time_delta, flags) = (p.growing()?, p.byte()?);

        bitcheck!(flags {
            DealFlags::Timestamp => self.prev.timestamp = cadd!(self.prev.timestamp, p.growing()?),
            DealFlags::DealId    => self.prev.deal_id   = cadd!(self.prev.deal_id,   p.growing()?),
            DealFlags::OrderId   => self.prev.order_id  = cadd!(self.prev.order_id,  p.leb()?),
            DealFlags::Price     => self.prev.price     = cadd!(self.prev.price,     p.leb()?),
            DealFlags::Amount    => self.prev.amount    = p.leb()?,
            DealFlags::OI        => self.prev.oi        = cadd!(self.prev.oi,        p.leb()?)
        });
        self.prev.side = (flags & 0x03).into();
        self.prev.frame_time_delta = frame_time_delta;
        Ok(self.prev.clone())
    }
}

// - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - AuxInfo
#[derive(Debug, Default)]
pub struct AuxInfoReader {
    prev: AuxInfo,
}

impl QshParser for AuxInfoReader {
    type Item = AuxInfo;

    fn parse(&mut self, p: &mut impl QshRead) -> Result<Self::Item, QshError> {
        let (frame_time_delta, flags) = (p.growing()?, p.byte()?);
        self.prev.frame_time_delta = frame_time_delta;

        bitcheck!(flags {
            AuxInfoFlags::Timestamp   => self.prev.timestamp = cadd!(self.prev.timestamp, p.growing()?),
            AuxInfoFlags::AskTotal    => self.prev.ask_total = cadd!(self.prev.ask_total, p.leb()?),
            AuxInfoFlags::BidTotal    => self.prev.bid_total = cadd!(self.prev.bid_total, p.leb()?),
            AuxInfoFlags::OI          => self.prev.oi        = cadd!(self.prev.oi,        p.leb()?),
            AuxInfoFlags::Price       => self.prev.price     = cadd!(self.prev.price,     p.leb()?),
            AuxInfoFlags::SessionInfo => { self.prev.hi_limit  = p.leb()?;
                                           self.prev.low_limit = p.leb()?;
                                           self.prev.deposit   = p.f64()?; },
            AuxInfoFlags::Rate        => self.prev.rate = p.f64()?
        });

        if AuxInfoFlags::Message % flags {
            self.prev.message = p.string()?;
        } else {
            self.prev.message.clear();
        }

        Ok(self.prev.clone())
    }
}
