### Python обёртка для qsh-rs

### Установка
`python wheel` собирается при помощи [maturin](https://github.com/PyO3/maturin).
```bash
pip install maturin

maturin build --release
pip install --force-reinstall target/wheels/pyqsh-*.whl
```

### Arch Linux package
```bash
pypi2pkgbuild.py -g cython -f file://target/wheels/pyqsh-*.whl
```

### Использование
```python
import pyqsh
import matplotlib.pyplot as plt

file = "../data/zerich/Si-3.20.2020-03-17.OrdLog.qsh"
depth = 5

lob = pyqsh.lob(file, depth)

print(type(lob))
# <class 'numpy.ndarray'>
print(lob.shape)
# (9758380, 21)

timestamp = lob[:,0]
mid_price = (lob[:,1] + lob[:,3]) * 0.5

plt.plot(mid_price)
plt.show()
```
