# qsh-rs
Парсер для QScalp формата 4й версии. 

Поддерживаемые потоки: OrderLog, Deals, Quotes, AuxInfo.

### [Python api](pyqsh)
Для сборки стакана из ордерлога и загрузки в numpy.

```bash
git clone https://github.com/2dav/qsh-rs
cd qsh-rs/pyqsh

pip install maturin

maturin build --release
pip install --force-reinstall target/wheels/pyqsh-*.whl
```
```python
import pyqsh

file = "../data/zerich/Si-3.20.2020-03-17.OrdLog.qsh"
depth = 5

lob = pyqsh.lob(file, depth)

print(type(lob)) # <class 'numpy.ndarray'>
print(lob.shape) # (9758380, 21)

timestamp = lob[:,0]
mid_price = (lob[:,1] + lob[:,3]) * 0.5
```
### Rust api
Cargo.toml
```
[dependencies]
qsh-rs = { git = "https://github.com/2dav/qsh-rs" }
```

#### Пример
```rust
use qsh_rs::{deflate, header, QshParser};
use qsh_rs::{AuxInfoReader, DealReader, OrderLogReader, QuotesReader};

let f = "data/zerich/Si-3.20.2020-03-17.OrdLog.qsh";
// Прочитать файл целиком, разжать
let bytes = deflate(f.into()).unwrap();
// Создать парсер для qsh-примитивов
let mut parser = QshParser::new(bytes);
// Прочитать qsh-заголовок
let header = header(&mut parser).unwrap();
println!("{:#?}", header);
```
```
Header {
    recording_time: 637200251900000000,
    version: 4,
    stream: ORDERLOG,
    instrument: "Plaza2:Si-3.20::1252209:1",
    recorder: "QshWriter.6870",
    comment: "Zerich QSH Service",
}
```
```rust
// ...и, наконец, конвертировать парсер в итератор соответствующего типа
let iter = parser.into_iter::<OrderLogReader>();
for rec in iter{
	//
}
```
### OrderBook 
Пример сборки стакана из ордерлога. 
```rust
use qsh_rs::orderbook::{self as ob, PartitionBy};
use qsh_rs::types::{Event, OLFlags, Side};
use qsh_rs::{deflate, header, OrderLogReader, QshParser, QshReader};

let bytes = deflate("data/zerich/Si-3.20.2020-03-17.OrdLog.qsh".into()).unwrap();
let mut parser = QshParser::new(bytes);
header(&mut parser).unwrap();

let mut book: ob::OrderBook = Default::default();

let iter = parser
    .into_iter::<OrderLogReader>()
    .filter(ob::system_record) // отфильтруем внесистемные сделки
    .partition_by(ob::tx_end) // сгруппируем в транзакции
    .filter(ob::fiok_with_trades); // отфильтруем IOK/FOK ордера без сделок 

for tx in iter {
    if OLFlags::NewSession % tx[0].order_flags {
        book.clear();
    }
    tx.into_iter().for_each(|r| match Event::from(&r) {
        Event::Add => book.add(r),
        Event::Fill => book.trade(r),
        Event::Cancel | Event::Remove => book.cancel(r),
        Event::UNKNOWN => unreachable!(),
    });

    if book.depth(Side::Buy) >= 3 && book.depth(Side::Sell) >= 3 {
        // срез стакана глубины 3
        // ts            [Pb, Vb, Ps, Vs, Pb-1, Vb-1, Ps+1, Vs+1 ... Pb-3, Vb-3, Ps+3, Vs+3]
        // 1584440657760 [73914, 5, 73916, 14, 73913, 4, 73917, 6, 73912, 95, 73920, 3]
        println!("{:?}", book.snapshot(3));

        println!("{}", book.mid_price());
    }
}
```
Код сборки утыкан assert'ами и проверками инвариантов для
всех ордеров, из целостных данных(например от Церих) стаканы собираются
без ошибок. 
