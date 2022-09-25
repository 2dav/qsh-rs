# qsh-rs
Парсер для [QScalp History](https://www.qscalp.ru/qsh-service) формата `4` версии. 

Поддерживаемые потоки: OrderLog, Deals, Quotes, AuxInfo.

- [Описание](#описание)
- [Rust api](#rust-api)
- [Примеры](#примеры)
- [Python api](#python-api)
- [L3toL2](#l3tol2)

### Описание
`qsh` файл состоит из бинарных потоков исторических рыночных данных, сжатых deflate/flate алгоритмом.

Потоки различаются структурой/состоянием в зависимости от типа рыночных данных, представленных в потоке,
а набор примитивов, используемый для кодирования данных, одинаков для всех типов потоков

 ```rust
pub trait QshRead: Read + Sized {
	...
    fn uleb(&mut self) -> Result<u64, QshError>;
    fn leb(&mut self) -> Result<i64, QshError>;
    fn growing(&mut self) -> Result<i64, QshError>;
    fn string(&mut self) -> Result<String, QshError>;
}
```
этот интерфейс реализован для всех типов, удовлетворяющих `std::io::BufRead`.

Чтение `qsh` файла начинается с декомпрессии, функция `qsh_rs::deflate` создаёт буферизированный reader-декодер,
использующий [flate2](https://docs.rs/flate2/latest/flate2/) для декомпрессии
```rust
let reader = qsh_rs::deflate(file_path)?;
```
В начале файла расположен заголовок, описывающий имеющиеся потоки и дополнительную информацию
```rust
qsh_rs::header(&mut reader)?
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
Далее идут инкрементальные данные потока —  reader нужно преобразовать в итератор соответствующего типа  
```rust
use qsh_rs::{AuxInfoReader, DealReader, OrderLogReader, QuotesReader}

reader.into_iter<OrderLogReader>(); // impl Iterator<Item = OrderLog>
reader.into_iter<QuotesReader>();	// impl Iterator<Item = Quotes>
reader.into_iter<DealReader>();		// impl Iterator<Item = Deal>
reader.into_iter<AuxInfoReader>();	// impl Iterator<Item = AuxInfo>
```
### Rust api
```toml
[dependencies]
qsh-rs = { git = "https://github.com/2dav/qsh-rs" }
```
```rust
use qsh_rs::{deflate, header, QshRead};
use qsh_rs::{AuxInfoReader, DealReader, OrderLogReader, QuotesReader};

```
### Примеры
`examples/l3book.rs`
сборка стакана из L3(OrderLog) потока
> cargo run --release --example l3book

`examples/l2book.rs`
стакан из L2(Quotes) потока 
> cargo run --release --example l2book

### [Python api](tools/pyqsh)
- [Установка](tools/pyqsh)

сборка стакана из ордерлога и загрузка в `numpy`

```python
import pyqsh

file = "../../data/zerich/Si-3.20.2020-03-17.OrdLog.qsh"
depth = 5

lob = pyqsh.lob(file, depth)

print(type(lob)) # <class 'numpy.ndarray'>
print(lob.shape) # (9758380, 21)

timestamp = lob[:,0]
mid_price = (lob[:,1] + lob[:,3]) * 0.5
```

### L3toL2
Конвертер L3(orderlog) событий в l2(quotes) события

```bash
cd tools/l3tol2
cargo build --release
target/release/l3tol2 --help
```
