use bincode::{Decode, Encode};
use std::ops::Rem;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Stream {
    QUOTES,
    DEALS,
    OWNORDERS,
    OWNTRADES,
    MESSAGES,
    AUXINFO,
    ORDERLOG,
}

impl From<u8> for Stream {
    fn from(v: u8) -> Self {
        match v {
            0x10 => Stream::QUOTES,
            0x20 => Stream::DEALS,
            0x60 => Stream::AUXINFO,
            0x70 => Stream::ORDERLOG,
            _ => panic!("Неподдерживаемый тип потока: {}", v),
        }
    }
}

#[derive(Debug)]
pub struct Header {
    pub recording_time: i64,
    pub version: u8,
    pub stream: Stream,
    pub instrument: String,
    pub recorder: String,
    pub comment: String,
}

#[derive(PartialEq, Debug, Copy, Clone, Encode, Decode)]
pub enum Side {
    Buy = 1,
    Sell = 2,
    UNKNOWN = 0,
}

impl Default for Side {
    fn default() -> Self {
        Side::UNKNOWN
    }
}

impl From<u8> for Side {
    fn from(b: u8) -> Self {
        match b {
            1 => Side::Buy,
            2 => Side::Sell,
            _ => Side::UNKNOWN,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum OrderType {
    Limit,
    IOK,
    FOK,
    UNKNOWN,
}
impl Default for OrderType {
    fn default() -> Self {
        OrderType::UNKNOWN
    }
}

impl From<u16> for OrderType {
    fn from(order_flags: u16) -> Self {
        if OLFlags::Counter % order_flags {
            OrderType::IOK
        } else if OLFlags::FillOrKill % order_flags {
            OrderType::FOK
        } else if OLFlags::Quote % order_flags {
            OrderType::Limit
        } else {
            unreachable!("Неизвестный тип ордера: {}", order_flags);
        }
    }
}

macro_rules! flags {
    ($name:ident $($k:ident = $v:expr),*) => {flags!($name u8 $($k = $v),*);};
    ($name:ident $t:ident $($k:ident = $v:expr),*) => {
        #[repr($t)]
        pub enum $name {
            $($k = $v,)*
        }
        impl Rem<$t> for $name {
            type Output = bool;
            fn rem(self, rhs: $t) -> Self::Output {
                (self as $t & rhs) > 0
            }
        }
    };}

flags!(DealFlags
    Timestamp   = 1 << 2,   // биржевые дата и время сделки
    DealId      = 1 << 3,   // номер сделки в торговой системе
    OrderId     = 1 << 4,   // номер заявки, по которой была совершена данная сделка
    Price       = 1 << 5,   // цена сделки
    Amount      = 1 << 6,   // объем сделки
    OI          = 1 << 7    // открытый интерес по инструменту после совершения сделки
);
flags!(AuxInfoFlags
    Timestamp   = 1,        // биржевое время обновления данных
    AskTotal    = 1 << 1,   // суммарный объем котировок «ask»
    BidTotal    = 1 << 2,   // суммарный объем котировок «bid»
    OI          = 1 << 3,   // количество открытых позиций
    Price       = 1 << 4,   // цена последней сделки инструменту
    SessionInfo = 1 << 5,   // информация о сессии: верхний лимит цены, нижний лимит цены, гарантийное обеспечение
    Rate        = 1 << 6,   // курс пересчета пунктов инструмента в денежные единицы
    Message     = 1 << 7    // сообщение торговой системы
);
flags!(OLEntryFlags
    DateTime    = 1,        // биржевое время обновления данных
    OrderId     = 1 << 1,   // номер заявки в торговой системе
    Price       = 1 << 2,   // цена в заявке
    Amount      = 1 << 3,   // количество инструмента в данной операции
    AmountRest  = 1 << 4,   // остаток в заявке
    DealId      = 1 << 5,   // номер сделки, в которую сведена заявка
    DealPrice   = 1 << 6,   // цена сделки, в которую была сведена заявка
    OI          = 1 << 7    // открытый интерес после заключения сделки
);
flags!(OLFlags u16
    NonZeroReplAct  = 1,        // при получении данной записи поле ReplAct не было равно нулю
    NewSession      = 1 << 1,   // данная запись получена с новым идентификатором сессии или после сообщения смены номера жизни потока
    Add             = 1 << 2,   // новая заявка
    Fill            = 1 << 3,   // заявка сведена в сделку
    Buy             = 1 << 4,   // покупка
    Sell            = 1 << 5,   // продажа
    Snapshot        = 1 << 6,   // запись получена из архива торговой системы
    Quote           = 1 << 7,   // Котировочная
    Counter         = 1 << 8,   // Встречная
    NonSystem       = 1 << 9,   // Внесистемная
    TxEnd           = 1 << 10,  // Запись является последней в транзакции
    FillOrKill      = 1 << 11,  // Заявка Fill-or-kill
    Moved           = 1 << 12,  // Запись является результатом операции перемещения заявки
    Canceled        = 1 << 13,  // Запись является результатом операции удаления заявки
    CanceledGroup   = 1 << 14,  // Запись является результатом группового удаления
    CrossTrade      = 1 << 15   // Признак удаления остатка заявки по причине кросс-сделки
);

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Event {
    Add,
    Fill,
    Cancel,
    Remove,
    UNKNOWN,
}

impl Default for Event {
    fn default() -> Self {
        Event::UNKNOWN
    }
}

impl From<&OrderLog> for Event {
    fn from(r: &OrderLog) -> Event {
        if OLFlags::Add % r.order_flags {
            Event::Add
        } else if OLFlags::Fill % r.order_flags {
            Event::Fill
        } else if OLFlags::Canceled % r.order_flags
            || OLFlags::CanceledGroup % r.order_flags
            || OLFlags::Moved % r.order_flags
        {
            Event::Cancel
        } else if OLFlags::CrossTrade % r.order_flags || r.amount_rest == 0 {
            Event::Remove
        } else {
            unreachable!("Ошибка в логике программы или корявый ордер \n{}", r);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OrderLog {
    pub frame_time_delta: i64,
    pub timestamp: i64,
    pub order_id: i64,
    pub price: i64,
    pub amount: i64,
    pub amount_rest: i64,
    pub deal_id: i64,
    pub deal_price: i64,
    pub oi: i64,
    pub order_flags: u16,
    pub entry_flags: u8,
    pub side: Side,
    pub event: Event,
    pub type_: OrderType,
}

impl std::fmt::Display for OrderLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:#?}, {:?}, {:#?}",
            self,
            OrderType::from(self.order_flags),
            (
                OLFlags::Add % self.order_flags,
                OLFlags::Fill % self.order_flags,
                OLFlags::Moved % self.order_flags,
                OLFlags::Counter % self.order_flags,
                OLFlags::FillOrKill % self.order_flags,
                OLFlags::NewSession % self.order_flags,
                OLFlags::Canceled % self.order_flags,
                OLFlags::CanceledGroup % self.order_flags,
                OLFlags::CrossTrade % self.order_flags,
                OLFlags::TxEnd % self.order_flags,
            )
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct Quotes {
    pub frame_time_delta: i64,
    pub bid: Vec<(u64, u64)>,
    pub ask: Vec<(u64, u64)>,
}

#[derive(Debug, Default, Clone)]
pub struct Deal {
    pub frame_time_delta: i64,
    pub side: Side,
    pub timestamp: i64,
    pub deal_id: i64,
    pub order_id: i64,
    pub price: i64,
    pub amount: i64,
    pub oi: i64,
}

#[derive(Debug, Default, Clone)]
pub struct AuxInfo {
    pub frame_time_delta: i64,
    pub timestamp: i64,
    pub price: i64,
    pub ask_total: i64,
    pub bid_total: i64,
    pub oi: i64,
    pub hi_limit: i64,
    pub low_limit: i64,
    pub deposit: f64,
    pub rate: f64,
    pub message: String,
}
